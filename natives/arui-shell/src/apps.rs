//! Start menu app index: `.lnk` files from the user and common Programs folders plus packaged
//! apps from `shell:AppsFolder`, with 64px icons in the [`IconStore`].
//!
//! Runs on its own STA thread. The Programs folders are watched with
//! FindFirstChangeNotificationW (debounced); [`AppsThread::refresh`] forces a re-index.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use arui_win::util::{wide, ComGuard};
use arui_win::{AppEntry, AppKind, Error, IconStore, Result, ShellEvent};
use windows::core::{Interface, GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, SIZE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
    FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{
    CoCreateInstance, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ,
};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{
    IEnumShellItems, IShellItem, IShellItem2, IShellItemImageFactory, IShellLinkW,
    SHCreateItemFromParsingName, SHGetKnownFolderItem, SHGetKnownFolderPath, ShellLink,
    BHID_EnumItems, FOLDERID_AppsFolder, FOLDERID_CommonPrograms, FOLDERID_Programs,
    KF_FLAG_DEFAULT, SIGDN_NORMALDISPLAY, SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT,
};

use crate::Inner;

const ICON_SIZE: i32 = 64;
const DEBOUNCE: Duration = Duration::from_millis(1500);

// ------------------------------------------------------------------ pure helpers

/// Links that clutter a start menu: uninstallers, help files, readmes, web links.
pub(crate) fn should_skip_link(name: &str, target: Option<&str>) -> bool {
    let n = name.to_lowercase();
    let words: Vec<&str> = n.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    const WORDS: &[&str] = &[
        "uninstall", "uninstaller", "uninst", "deinstall", "readme", "help", "documentation",
        "manual", "website", "license", "changelog", "faq",
    ];
    const PHRASES: &[&str] = &["release notes", "read me", "what's new", "user guide"];
    if words.iter().any(|w| WORDS.contains(w)) || PHRASES.iter().any(|p| n.contains(p)) {
        return true;
    }
    if let Some(t) = target {
        let t = t.to_lowercase();
        let file = t.rsplit(['\\', '/']).next().unwrap_or(&t);
        const EXTS: &[&str] = &[".chm", ".hlp", ".txt", ".pdf", ".htm", ".html", ".url", ".rtf", ".md"];
        if EXTS.iter().any(|e| file.ends_with(e)) {
            return true;
        }
        if file.starts_with("unins") || file.contains("uninstall") {
            return true;
        }
    }
    false
}

/// Packaged app ids look like `PackageFamilyName!AppId`.
pub(crate) fn is_packaged_aumid(s: &str) -> bool {
    match s.split_once('!') {
        Some((pfn, app)) => !pfn.is_empty() && !app.is_empty() && pfn.contains('_'),
        None => false,
    }
}

/// Start menu sub folder of `link` relative to `root`, using `/` as separator.
pub(crate) fn folder_of(root: &Path, link: &Path) -> Option<String> {
    let parent = link.parent()?.strip_prefix(root).ok()?;
    let parts: Vec<String> = parent.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

/// Top-down BGRA (straight or premultiplied alpha) to straight RGBA, in place.
/// Treats an all-zero alpha channel as fully opaque.
pub(crate) fn bgra_to_rgba(px: &mut [u8]) {
    let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
    let premultiplied =
        has_alpha && px.chunks_exact(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
        if !has_alpha {
            p[3] = 255;
        } else if premultiplied && p[3] != 0 && p[3] != 255 {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// Drops later entries with the same id, and shortcuts with the same name + exe as an earlier one
/// (the same app linked in both the user and the common folder).
pub(crate) fn dedupe(entries: Vec<AppEntry>) -> Vec<AppEntry> {
    let mut ids = HashSet::new();
    let mut name_exe = HashSet::new();
    let mut out = Vec::new();
    for e in entries {
        if !ids.insert(e.id.clone()) {
            continue;
        }
        if let Some(exe) = &e.exe_path {
            if !name_exe.insert((e.name.to_lowercase(), exe.to_lowercase())) {
                continue;
            }
        }
        out.push(e);
    }
    out
}

// ------------------------------------------------------------------ win32 helpers

unsafe fn take_pwstr(p: PWSTR) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as *const _));
    s.filter(|s| !s.is_empty())
}

fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok().and_then(|p| take_pwstr(p)) }
        .map(PathBuf::from)
}

pub(crate) fn program_folders() -> Vec<PathBuf> {
    [FOLDERID_Programs, FOLDERID_CommonPrograms].iter().filter_map(known_folder).collect()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => walk(&p, out),
            Ok(_) if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) => out.push(p),
            _ => {}
        }
    }
}

/// (target exe, AppUserModelID) of a shortcut.
fn read_link(path: &Path) -> Result<(Option<String>, Option<String>)> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let pf: IPersistFile = link.cast()?;
        let w = wide(&path.to_string_lossy());
        pf.Load(PCWSTR(w.as_ptr()), STGM_READ)?;
        let mut buf = [0u16; 1024];
        let exe = match link.GetPath(&mut buf, std::ptr::null_mut(), 0) {
            Ok(()) => Some(arui_win::util::from_wide(&buf)).filter(|s| !s.is_empty()),
            Err(_) => None,
        };
        let aumid = link.cast::<IPropertyStore>().ok().and_then(|store| {
            let mut pv = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
            let s = PropVariantToStringAlloc(&pv).ok().and_then(|p| take_pwstr(p));
            let _ = PropVariantClear(&mut pv);
            s
        });
        Ok((exe, aumid))
    }
}

/// Reads an HBITMAP from IShellItemImageFactory as PNG.
fn hbitmap_to_png(hbm: HBITMAP) -> Result<Vec<u8>> {
    unsafe {
        let mut bm = BITMAP::default();
        if GetObjectW(HGDIOBJ(hbm.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0
            || bm.bmWidth <= 0
            || bm.bmHeight <= 0
        {
            return Err(Error::Other("GetObjectW failed".into()));
        }
        let (w, h) = (bm.bmWidth as u32, bm.bmHeight.unsigned_abs());
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        let dc = CreateCompatibleDC(None);
        let lines = GetDIBits(dc, hbm, 0, h, Some(px.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
        let _ = DeleteDC(dc);
        if lines == 0 {
            return Err(Error::Other("GetDIBits failed".into()));
        }
        bgra_to_rgba(&mut px);
        arui_win::icons::rgba_to_png(w, h, &px)
    }
}

fn item_icon(item: &IShellItem) -> Result<Vec<u8>> {
    unsafe {
        let f: IShellItemImageFactory = item.cast()?;
        let hbm = f.GetImage(SIZE { cx: ICON_SIZE, cy: ICON_SIZE }, SIIGBF_RESIZETOFIT | SIIGBF_ICONONLY)?;
        let r = hbitmap_to_png(hbm);
        let _ = DeleteObject(HGDIOBJ(hbm.0));
        r
    }
}

fn shell_item(path: &Path) -> Result<IShellItem> {
    let w = wide(&path.to_string_lossy());
    Ok(unsafe { SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None)? })
}

fn put_icon(icons: &IconStore, key: &str, f: impl FnOnce() -> Result<Vec<u8>>) {
    if icons.contains(key) {
        return;
    }
    match f() {
        Ok(png) => icons.put(key.to_string(), png),
        Err(e) => log::debug!("icon {key}: {e}"),
    }
}

// ------------------------------------------------------------------ indexing

fn shortcuts(icons: &IconStore) -> Vec<AppEntry> {
    let mut out = Vec::new();
    for root in program_folders() {
        let mut links = Vec::new();
        walk(&root, &mut links);
        for link in links {
            let stem = link.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let (exe, aumid) = read_link(&link).unwrap_or((None, None));
            if stem.is_empty() || should_skip_link(&stem, exe.as_deref()) {
                continue;
            }
            let item = shell_item(&link).ok();
            // The shell display name honours desktop.ini localization (e.g. "Komut İstemi").
            let name = item
                .as_ref()
                .and_then(|i| unsafe { i.GetDisplayName(SIGDN_NORMALDISPLAY) }.ok())
                .and_then(|p| unsafe { take_pwstr(p) })
                .unwrap_or(stem);
            let target = link.to_string_lossy().into_owned();
            let id = target.to_lowercase();
            let icon_key = format!("app:{id}");
            if let Some(item) = &item {
                put_icon(icons, &icon_key, || item_icon(item));
            }
            out.push(AppEntry {
                id,
                name,
                kind: AppKind::Shortcut,
                target,
                exe_path: exe,
                app_id: aumid,
                icon_key,
                folder: folder_of(&root, &link),
            });
        }
    }
    out
}

fn packaged(icons: &IconStore, known_aumids: &HashSet<String>) -> Result<Vec<AppEntry>> {
    let mut out = Vec::new();
    unsafe {
        let folder: IShellItem = SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)?;
        let en: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems)?;
        loop {
            let mut items = [None];
            let mut fetched = 0u32;
            if en.Next(&mut items, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = items[0].take() else { break };
            let Ok(item2) = item.cast::<IShellItem2>() else { continue };
            let Some(aumid) = item2.GetString(&PKEY_AppUserModel_ID).ok().and_then(|p| take_pwstr(p)) else {
                continue;
            };
            if !is_packaged_aumid(&aumid) || known_aumids.contains(&aumid.to_lowercase()) {
                continue;
            }
            let Some(name) = item.GetDisplayName(SIGDN_NORMALDISPLAY).ok().and_then(|p| take_pwstr(p)) else {
                continue;
            };
            let icon_key = format!("app:{aumid}");
            put_icon(icons, &icon_key, || item_icon(&item));
            out.push(AppEntry {
                id: aumid.clone(),
                name,
                kind: AppKind::Packaged,
                target: aumid.clone(),
                exe_path: None,
                app_id: Some(aumid),
                icon_key,
                folder: None,
            });
        }
    }
    Ok(out)
}

/// Full index; COM must be initialized on the calling thread.
pub(crate) fn index(icons: &IconStore) -> Vec<AppEntry> {
    let mut all = dedupe(shortcuts(icons));
    let known: HashSet<String> = all.iter().filter_map(|e| e.app_id.as_ref().map(|a| a.to_lowercase())).collect();
    match packaged(icons, &known) {
        Ok(p) => all.extend(p),
        Err(e) => log::warn!("packaged app enumeration failed: {e}"),
    }
    let mut all = dedupe(all);
    all.sort_by_key(|a| a.name.to_lowercase());
    all
}

// ------------------------------------------------------------------ thread

struct Handle(HANDLE);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(crate) struct AppsThread {
    stop: Arc<Handle>,
    refresh: Arc<Handle>,
    join: Option<JoinHandle<()>>,
}

impl AppsThread {
    pub fn spawn(inner: Arc<Inner>) -> Result<Self> {
        let stop = Arc::new(Handle(unsafe { CreateEventW(None, true, false, PCWSTR::null())? }));
        let refresh = Arc::new(Handle(unsafe { CreateEventW(None, false, false, PCWSTR::null())? }));
        let (s, r) = (stop.clone(), refresh.clone());
        let join = std::thread::Builder::new()
            .name("arui-shell-apps".into())
            .spawn(move || run(inner, s, r))
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(Self { stop, refresh, join: Some(join) })
    }

    pub fn refresh(&self) {
        unsafe {
            let _ = SetEvent(self.refresh.0);
        }
    }
}

impl Drop for AppsThread {
    fn drop(&mut self) {
        unsafe {
            let _ = SetEvent(self.stop.0);
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn reindex(inner: &Inner) {
    let apps = index(&inner.icons);
    let changed = {
        let mut st = inner.st.lock();
        let changed = !st.apps_indexed || st.apps != apps;
        st.apps = apps.clone();
        st.apps_indexed = true;
        changed
    };
    log::info!("indexed {} start menu apps", apps.len());
    if changed {
        inner.emit(ShellEvent::AppsChanged(apps));
    }
}

fn run(inner: Arc<Inner>, stop: Arc<Handle>, refresh: Arc<Handle>) {
    let _com = ComGuard::sta();
    reindex(&inner);

    let watches: Vec<Handle> = program_folders()
        .iter()
        .filter_map(|p| {
            let w = wide(&p.to_string_lossy());
            unsafe {
                FindFirstChangeNotificationW(
                    PCWSTR(w.as_ptr()),
                    true,
                    FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE,
                )
            }
            .ok()
            .map(Handle)
        })
        .collect();
    let close_watches = |ws: Vec<Handle>| {
        for w in ws {
            unsafe {
                let _ = FindCloseChangeNotification(w.0);
            }
            std::mem::forget(w);
        }
    };

    let mut handles = vec![stop.0, refresh.0];
    handles.extend(watches.iter().map(|w| w.0));
    loop {
        let r = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) };
        let idx = r.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
        match idx {
            0 => break,
            1 => reindex(&inner),
            i if i < handles.len() => {
                unsafe {
                    let _ = FindNextChangeNotification(handles[i]);
                }
                // Debounce: installers touch many files; wait until things are quiet.
                let mut stopped = false;
                loop {
                    let mut wait = vec![stop.0];
                    wait.extend(watches.iter().map(|w| w.0));
                    let r = unsafe { WaitForMultipleObjects(&wait, false, DEBOUNCE.as_millis() as u32) };
                    if r == WAIT_TIMEOUT {
                        break;
                    }
                    let j = r.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
                    if j == 0 {
                        stopped = true;
                        break;
                    }
                    if j < wait.len() {
                        unsafe {
                            let _ = FindNextChangeNotification(wait[j]);
                        }
                    } else {
                        break;
                    }
                }
                if stopped {
                    break;
                }
                reindex(&inner);
            }
            _ => {
                log::warn!("apps watcher wait failed");
                break;
            }
        }
    }
    close_watches(watches);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_filter() {
        assert!(should_skip_link("Uninstall Foo", Some(r"C:\Foo\unins000.exe")));
        assert!(should_skip_link("Foo", Some(r"C:\Foo\unins000.exe")));
        assert!(should_skip_link("Foo Help", Some(r"C:\Foo\foo.chm")));
        assert!(should_skip_link("Foo Website", None));
        assert!(should_skip_link("Release Notes", None));
        assert!(should_skip_link("Readme", Some(r"C:\Foo\readme.txt")));
        assert!(should_skip_link("Foo", Some(r"C:\Foo\manual.pdf")));
        assert!(!should_skip_link("Notepad++", Some(r"C:\Program Files\Notepad++\notepad++.exe")));
        assert!(!should_skip_link("Helper Tool", Some(r"C:\x\helper.exe")));
        assert!(!should_skip_link("Visual Studio Code", None));
        assert!(!should_skip_link("Windows PowerShell ISE", Some(r"C:\Windows\powershell_ise.exe")));
    }

    #[test]
    fn aumid_detection() {
        assert!(is_packaged_aumid("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"));
        assert!(!is_packaged_aumid("Chrome"));
        assert!(!is_packaged_aumid("Microsoft.Windows.Explorer"));
        assert!(!is_packaged_aumid("!App"));
    }

    #[test]
    fn folders() {
        let root = Path::new(r"C:\Start\Programs");
        assert_eq!(folder_of(root, Path::new(r"C:\Start\Programs\a.lnk")), None);
        assert_eq!(folder_of(root, Path::new(r"C:\Start\Programs\Accessories\a.lnk")).as_deref(), Some("Accessories"));
        assert_eq!(
            folder_of(root, Path::new(r"C:\Start\Programs\Accessories\System Tools\a.lnk")).as_deref(),
            Some("Accessories/System Tools")
        );
    }

    #[test]
    fn pixel_conversion() {
        // Premultiplied half-transparent red (BGRA): 128 red at alpha 128 -> 255 straight.
        let mut px = vec![0, 0, 128, 128, 0, 0, 0, 0];
        bgra_to_rgba(&mut px);
        assert_eq!(px, vec![255, 0, 0, 128, 0, 0, 0, 0]);
        // Straight alpha (a channel exceeds alpha): only swapped.
        let mut px = vec![10, 20, 200, 100];
        bgra_to_rgba(&mut px);
        assert_eq!(px, vec![200, 20, 10, 100]);
        // No alpha at all: opaque.
        let mut px = vec![1, 2, 3, 0];
        bgra_to_rgba(&mut px);
        assert_eq!(px, vec![3, 2, 1, 255]);
    }

    fn entry(id: &str, name: &str, exe: Option<&str>) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: name.into(),
            kind: AppKind::Shortcut,
            target: id.into(),
            exe_path: exe.map(Into::into),
            app_id: None,
            icon_key: format!("app:{id}"),
            folder: None,
        }
    }

    #[test]
    fn dedupes() {
        let v = dedupe(vec![
            entry("a", "Foo", Some(r"C:\foo.exe")),
            entry("a", "Foo again", None),
            entry("b", "foo", Some(r"c:\FOO.exe")),
            entry("c", "Bar", None),
        ]);
        assert_eq!(v.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["a", "c"]);
    }
}
