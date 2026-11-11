//! Owns the native services and the surface windows.

use std::sync::Arc;

use arui_win::{EventSink, IconStore, MonitorInfo, NativeEvent, Rect, ShellEvent};
use parking_lot::Mutex;
use serde_json::{json, Value};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::settings::Settings;
use crate::surfaces::{self, OpenArgs, SurfaceSpec, SurfaceWindow, Surfaces};

pub struct Natives {
    // Drop order matters: tray first (gives icons back to explorer), shell last (restores taskbar).
    pub tray: Option<arui_tray::TrayHost>,
    pub windows: Option<arui_windows::WindowTracker>,
    pub media: Option<arui_media::Media>,
    pub system: Option<arui_system::System>,
    pub shell: Option<arui_shell::Shell>,
}

pub struct Host {
    pub icons: IconStore,
    pub settings: Settings,
    pub natives: Mutex<Option<Natives>>,
    pub surfaces: Mutex<Surfaces>,
    pub watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// Set once natives have started. Surfaces are only created after that (appbars need the shell).
    pub ready: std::sync::atomic::AtomicBool,
    /// Serializes surface window creation so concurrent opens can't build the same label twice.
    pub open_lock: tauri::async_runtime::Mutex<()>,
}

impl Host {
    pub fn new(icons: IconStore) -> Self {
        Self {
            icons,
            settings: Settings::load(crate::paths::settings_file()),
            natives: Mutex::new(None),
            surfaces: Mutex::new(Surfaces::default()),
            watcher: Mutex::new(None),
            ready: std::sync::atomic::AtomicBool::new(false),
            open_lock: tauri::async_runtime::Mutex::new(()),
        }
    }

    /// Runs `f` with the natives if they are running.
    pub fn with<T>(&self, f: impl FnOnce(&Natives) -> T) -> Option<T> {
        self.natives.lock().as_ref().map(f)
    }

    pub fn monitors(&self) -> Vec<MonitorInfo> {
        self.with(|n| n.shell.as_ref().map(|s| s.monitors()))
            .flatten()
            .unwrap_or_else(arui_shell::monitors::enumerate)
    }

    /// Stops every native service (restores explorer's taskbar and tray).
    pub fn shutdown(&self) {
        if let Some(n) = self.natives.lock().take() {
            let Natives { tray, windows, media, system, shell } = n;
            drop(tray);
            drop(windows);
            drop(media);
            drop(system);
            drop(shell);
        }
    }
}

fn setting_bool(settings: &Settings, key: &str, default: bool) -> bool {
    settings.get("core", key).and_then(|v| v.as_bool()).unwrap_or(default)
}

/// Starts natives on a background thread. Never on the UI thread: services block until their
/// first snapshot, and probing ARUI's own windows from their threads needs the UI thread pumping.
pub fn start_natives_async(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("arui-natives-start".into())
        .spawn(move || {
            start_natives(&app);
            app.state::<Host>().ready.store(true, std::sync::atomic::Ordering::SeqCst);
            log::info!("natives started");
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move { sync_auto_surfaces(&app2).await });
        })
        .expect("spawn natives thread");
}

/// Starts natives. Each service is optional: a failure is logged and the rest keep working.
fn start_natives(app: &AppHandle) {
    let host = app.state::<Host>();
    let handle = app.clone();
    let sink: EventSink = Arc::new(move |ev: NativeEvent| {
        let channel = ev.channel();
        if let NativeEvent::Shell(se) = &ev {
            on_shell_event(&handle, se);
        }
        let payload = serde_json::to_value(&ev)
            .ok()
            .and_then(|mut v| v.get_mut("event").map(Value::take))
            .unwrap_or(Value::Null);
        let _ = handle.emit(channel, payload);
    });

    let icons = host.icons.clone();
    let opts = arui_shell::ShellOptions {
        hide_explorer_taskbar: setting_bool(&host.settings, "hideExplorerTaskbar", true),
        capture_start_key: setting_bool(&host.settings, "captureStartKey", true),
    };
    let log_err = |name: &str, e: &arui_win::Error| log::error!("{name} failed to start: {e}");

    let shell = arui_shell::start(sink.clone(), icons.clone(), opts)
        .map_err(|e| log_err("shell", &e))
        .ok();
    if let Some(shell) = &shell {
        spawn_guard(shell.state_file());
    }
    let tray = if setting_bool(&host.settings, "trayHost", true) {
        arui_tray::start(sink.clone(), icons.clone()).map_err(|e| log_err("tray", &e)).ok()
    } else {
        None
    };
    let windows = arui_windows::start(sink.clone(), icons.clone()).map_err(|e| log_err("windows", &e)).ok();
    let media = arui_media::start(sink.clone(), icons.clone()).map_err(|e| log_err("media", &e)).ok();
    let system = arui_system::start(sink).map_err(|e| log_err("system", &e)).ok();

    *host.natives.lock() = Some(Natives { tray, windows, media, system, shell });
}

/// The guard restores explorer's taskbar if ARUI dies without cleaning up.
fn spawn_guard(state_file: &std::path::Path) {
    let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("arui-guard.exe"))) else {
        return;
    };
    if !exe.is_file() {
        log::warn!("arui-guard.exe not found next to the executable; crash recovery disabled");
        return;
    }
    let res = std::process::Command::new(exe)
        .arg("--pid")
        .arg(std::process::id().to_string())
        .arg("--state-file")
        .arg(state_file)
        .spawn();
    if let Err(e) = res {
        log::warn!("failed to spawn arui-guard: {e}");
    }
}

fn on_shell_event(app: &AppHandle, e: &ShellEvent) {
    match e {
        ShellEvent::MonitorsChanged(_) | ShellEvent::ExplorerRestarted => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { sync_auto_surfaces(&app).await });
        }
        // StartKey is handled by the UI (core.start-menu in the host window), so mods can rebind it.
        _ => {}
    }
}

// ---------------------------------------------------------------- surfaces

fn spec_of(app: &AppHandle, id: &str) -> Result<SurfaceSpec, String> {
    app.state::<Host>()
        .surfaces
        .lock()
        .specs
        .get(id)
        .cloned()
        .ok_or_else(|| format!("unknown surface {id:?}"))
}

/// Replaces the surface specs, closes windows of removed/changed surfaces and opens auto surfaces.
pub async fn define_surfaces(app: &AppHandle, specs: Vec<SurfaceSpec>) {
    log::info!("surfaces_define: {:?}", specs.iter().map(|s| &s.id).collect::<Vec<_>>());
    let stale: Vec<String> = {
        let host = app.state::<Host>();
        let mut s = host.surfaces.lock();
        s.specs = specs.into_iter().map(|sp| (sp.id.clone(), sp)).collect();
        let specs = s.specs.clone();
        s.windows
            .iter()
            .filter(|(_, w)| specs.get(&w.surface) != Some(&w.spec))
            .map(|(l, _)| l.clone())
            .collect()
    };
    for label in stale {
        destroy_window(app, &label);
    }
    sync_auto_surfaces(app).await;
}

/// Opens auto-open surfaces (appbars on every monitor) and closes appbars on vanished monitors.
pub async fn sync_auto_surfaces(app: &AppHandle) {
    let host = app.state::<Host>();
    if !host.ready.load(std::sync::atomic::Ordering::SeqCst) {
        return; // called again once natives are up
    }
    let monitors = host.monitors();
    let ids: Vec<String> = monitors.iter().map(|m| m.id.clone()).collect();
    let (specs, orphaned): (Vec<SurfaceSpec>, Vec<String>) = {
        let s = host.surfaces.lock();
        let orphaned = s
            .windows
            .iter()
            .filter(|(_, w)| w.monitor.as_ref().is_some_and(|m| !ids.contains(m)))
            .map(|(l, _)| l.clone())
            .collect();
        (s.specs.values().filter(|sp| sp.window.auto_open()).cloned().collect(), orphaned)
    };
    for label in orphaned {
        destroy_window(app, &label);
    }
    for spec in specs {
        let targets: Vec<Option<String>> = if spec.window.per_monitor() {
            ids.iter().cloned().map(Some).collect()
        } else {
            vec![None]
        };
        for monitor in targets {
            let args = OpenArgs { id: spec.id.clone(), monitor, ..Default::default() };
            if let Err(e) = open_surface(app, args).await {
                log::warn!("auto-open {}: {e}", spec.id);
            }
        }
    }
}

fn destroy_window(app: &AppHandle, label: &str) {
    let host = app.state::<Host>();
    host.surfaces.lock().windows.remove(label);
    if let Some(w) = app.get_webview_window(label) {
        if let Ok(hwnd) = w.hwnd() {
            host.with(|n| {
                if let Some(s) = &n.shell {
                    s.unregister_appbar(hwnd.0 as usize as u64);
                }
            });
        }
        let _ = w.destroy();
    }
}

fn monitor_for(host: &Host, app: &AppHandle, wanted: Option<&str>) -> Result<MonitorInfo, String> {
    let monitors = host.monitors();
    if let Some(id) = wanted {
        return monitors.into_iter().find(|m| m.id == id).ok_or_else(|| format!("unknown monitor {id:?}"));
    }
    let (cx, cy) = cursor(app);
    monitors
        .iter()
        .find(|m| cx >= m.bounds.left && cx < m.bounds.right && cy >= m.bounds.top && cy < m.bounds.bottom)
        .or_else(|| monitors.iter().find(|m| m.primary))
        .or(monitors.first())
        .cloned()
        .ok_or_else(|| "no monitors".to_string())
}

fn cursor(app: &AppHandle) -> (i32, i32) {
    app.cursor_position().map(|p| (p.x as i32, p.y as i32)).unwrap_or((0, 0))
}

fn build_window(app: &AppHandle, label: &str, spec: &SurfaceSpec, monitor: Option<&str>) -> Result<WebviewWindow, String> {
    let o = &spec.window;
    let url = WebviewUrl::App(surfaces::url(&spec.id, monitor).into());
    let win = WebviewWindowBuilder::new(app, label, url)
        .title(format!("ARUI {}", spec.id))
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .focusable(o.focusable())
        .build()
        .map_err(|e| e.to_string())?;

    if o.close_on_blur() {
        let handle = app.clone();
        let l = label.to_string();
        win.on_window_event(move |e| {
            if let WindowEvent::Focused(false) = e {
                if let Some(w) = handle.get_webview_window(&l) {
                    handle.state::<Host>().surfaces.lock().blurred.insert(l.clone(), std::time::Instant::now());
                    let _ = w.hide();
                    let _ = w.emit_to(w.label(), "arui://surface-hidden", json!({}));
                }
            }
        });
    }
    Ok(win)
}

/// Runs `f` on the main (UI) thread and waits for the result.
async fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce(&AppHandle) -> T + Send + 'static) -> Result<T, String> {
    let (tx, rx) = tauri::async_runtime::channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.try_send(f(&handle));
    })
    .map_err(|e| e.to_string())?;
    let mut rx = rx;
    rx.recv().await.ok_or_else(|| "main thread task dropped".to_string())
}

pub async fn open_surface(app: &AppHandle, args: OpenArgs) -> Result<(), String> {
    let spec = spec_of(app, &args.id)?;
    let host = app.state::<Host>();
    let _guard = host.open_lock.lock().await;
    let monitor = monitor_for(&host, app, args.monitor.as_deref())?;
    let per_monitor = spec.window.per_monitor();
    let monitor_id = per_monitor.then(|| monitor.id.clone());
    let label = surfaces::label(&spec.id, monitor_id.as_deref());
    let params = Value::Object(args.params.clone().unwrap_or_default());

    let existing = app.get_webview_window(&label);
    let is_new = existing.is_none();
    let win = match existing {
        Some(w) => w,
        None => {
            let (l, s, m) = (label.clone(), spec.clone(), monitor_id.clone());
            on_main(app, move |app| build_window(app, &l, &s, m.as_deref())).await??
        }
    };
    host.surfaces.lock().windows.insert(
        label.clone(),
        SurfaceWindow { surface: spec.id.clone(), monitor: monitor_id.clone(), params: params.clone(), spec: spec.clone() },
    );

    let o = &spec.window;
    if o.is_appbar() {
        if is_new {
            let thickness = (o.thickness() * monitor.scale).round() as i32;
            let (edge, reserve, mid) = (o.edge(), o.reserve(), monitor.id.clone());
            let w = win.clone();
            let rect = on_main(app, move |app| -> Result<Rect, String> {
                let hwnd = w.hwnd().map_err(|e| e.to_string())?.0 as usize as u64;
                let host = app.state::<Host>();
                let res = host.with(|n| n.shell.as_ref().map(|s| s.register_appbar(hwnd, &mid, edge, thickness, reserve)));
                match res.flatten() {
                    Some(r) => r.map_err(|e| e.to_string()),
                    None => Err("shell not running".into()),
                }
            })
            .await?;
            match rect {
                Ok(r) => place_window(&win, r),
                Err(e) => {
                    // Fallback without the shell: just put the bar at the monitor edge.
                    log::warn!("appbar registration failed ({e}); placing window without reservation");
                    let t = (o.thickness() * monitor.scale).round() as i32;
                    let b = monitor.bounds;
                    place_window(&win, Rect { left: b.left, top: b.bottom - t, right: b.right, bottom: b.bottom });
                }
            }
        }
    } else {
        place_window(&win, surfaces::place(o, &monitor, cursor(app), args.rect));
    }

    win.show().map_err(|e| e.to_string())?;
    if o.focusable() && !o.is_appbar() {
        let _ = win.set_focus();
    }
    let _ = win.emit_to(win.label(), "arui://surface-shown", json!({ "params": params }));
    Ok(())
}

fn place_window(win: &WebviewWindow, r: Rect) {
    let _ = win.set_size(PhysicalSize::new(r.width().max(1) as u32, r.height().max(1) as u32));
    let _ = win.set_position(PhysicalPosition::new(r.left, r.top));
}

fn labels_for(app: &AppHandle, id: &str, monitor: Option<&str>) -> Vec<String> {
    let host = app.state::<Host>();
    let s = host.surfaces.lock();
    s.windows
        .iter()
        .filter(|(_, w)| w.surface == id && (monitor.is_none() || w.monitor.as_deref() == monitor))
        .map(|(l, _)| l.clone())
        .collect()
}

pub fn close_surface(app: &AppHandle, id: &str, monitor: Option<&str>) {
    for label in labels_for(app, id, monitor) {
        let host = app.state::<Host>();
        let appbar = host.surfaces.lock().windows.get(&label).is_some_and(|w| w.spec.window.is_appbar());
        if appbar {
            // Appbars release their reservation when destroyed.
            destroy_window(app, &label);
        } else if let Some(w) = app.get_webview_window(&label) {
            let _ = w.hide();
            let _ = w.emit_to(w.label(), "arui://surface-hidden", json!({}));
        }
    }
}

pub async fn toggle_surface(app: &AppHandle, args: OpenArgs) -> Result<(), String> {
    let labels = labels_for(app, &args.id, args.monitor.as_deref());
    let just_blurred = {
        let host = app.state::<Host>();
        let s = host.surfaces.lock();
        labels.iter().any(|l| s.blurred.get(l).is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(300)))
    };
    if just_blurred {
        return Ok(());
    }
    let visible = labels
        .iter()
        .filter_map(|l| app.get_webview_window(l))
        .any(|w| w.is_visible().unwrap_or(false));
    if visible {
        close_surface(app, &args.id, args.monitor.as_deref());
        Ok(())
    } else {
        open_surface(app, args).await
    }
}
