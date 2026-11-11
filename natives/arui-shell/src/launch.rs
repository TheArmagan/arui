use arui_win::util::{wide, ComGuard};
use arui_win::Result;
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// `ShellExecuteExW(verb, path, args)`. Works for executables, documents, folders, URLs, `.lnk`
/// files and `shell:` paths. Blocks until the launch is handed off; any thread (initializes an
/// STA for the duration of the call when the thread has no COM apartment yet).
pub fn shell_execute(path: &str, args: Option<&str>, verb: Option<&str>) -> Result<()> {
    let _com = ComGuard::sta();
    let file = wide(path);
    let args = args.map(wide);
    let verb = verb.map(wide);
    let ptr = |v: &Option<Vec<u16>>| v.as_ref().map_or(PCWSTR::null(), |w| PCWSTR(w.as_ptr()));
    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        lpVerb: ptr(&verb),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: ptr(&args),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut sei)? };
    Ok(())
}
