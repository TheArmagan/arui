//! Tauri commands. Names and arguments match `Commands` in renderers/sdk/src/protocol.ts.

use arui_win::{AppEntry, Hwnd, MediaAction, MediaSession, MonitorInfo, NetworkState, Rect, TrayAction, TrayIcon, VolumeState, WindowInfo};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::host::{self, Host};
use crate::mods;
use crate::surfaces::{OpenArgs, SurfaceSpec};

type R<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn not_running(name: &str) -> String {
    format!("{name} service is not running")
}

macro_rules! svc {
    ($host:expr, $field:ident, |$s:ident| $body:expr) => {
        $host
            .with(|n| n.$field.as_ref().map(|$s| $body))
            .flatten()
            .ok_or_else(|| not_running(stringify!($field)))
    };
}

fn win_hwnd(w: &WebviewWindow) -> R<Hwnd> {
    Ok(w.hwnd().map_err(err)?.0 as usize as Hwnd)
}

// ---------------------------------------------------------------- windows

#[tauri::command]
pub fn windows_list(host: State<Host>) -> Vec<WindowInfo> {
    svc!(host, windows, |w| w.list()).unwrap_or_default()
}

#[tauri::command]
pub fn window_focus(host: State<Host>, hwnd: Hwnd) -> R<()> {
    svc!(host, windows, |w| w.focus(hwnd))?.map_err(err)
}
#[tauri::command]
pub fn window_toggle(host: State<Host>, hwnd: Hwnd) -> R<()> {
    svc!(host, windows, |w| w.toggle(hwnd))?.map_err(err)
}
#[tauri::command]
pub fn window_minimize(host: State<Host>, hwnd: Hwnd) -> R<()> {
    svc!(host, windows, |w| w.minimize(hwnd))?.map_err(err)
}
#[tauri::command]
pub fn window_maximize(host: State<Host>, hwnd: Hwnd) -> R<()> {
    svc!(host, windows, |w| w.maximize(hwnd))?.map_err(err)
}
#[tauri::command]
pub fn window_restore(host: State<Host>, hwnd: Hwnd) -> R<()> {
    svc!(host, windows, |w| w.restore(hwnd))?.map_err(err)
}
#[tauri::command]
pub fn window_close(host: State<Host>, hwnd: Hwnd) -> R<()> {
    svc!(host, windows, |w| w.close(hwnd))?.map_err(err)
}

/// `rect` is in CSS px relative to the calling window; DWM wants physical px.
#[tauri::command]
pub fn preview_show(window: WebviewWindow, host: State<Host>, hwnd: Hwnd, rect: Rect) -> R<()> {
    let scale = window.scale_factor().map_err(err)?;
    let s = |v: i32| (v as f64 * scale).round() as i32;
    let phys = Rect { left: s(rect.left), top: s(rect.top), right: s(rect.right), bottom: s(rect.bottom) };
    let dest = win_hwnd(&window)?;
    svc!(host, windows, |w| w.show_preview(dest, hwnd, phys))?.map_err(err)
}

#[tauri::command]
pub fn preview_hide(window: WebviewWindow, host: State<Host>, hwnd: Option<Hwnd>) -> R<()> {
    let dest = win_hwnd(&window)?;
    svc!(host, windows, |w| w.hide_preview(dest, hwnd))
}

// ---------------------------------------------------------------- apps

#[tauri::command]
pub fn apps_list(host: State<Host>) -> Vec<AppEntry> {
    svc!(host, shell, |s| s.apps()).unwrap_or_default()
}

#[tauri::command]
pub async fn app_launch(host: State<'_, Host>, id: String) -> R<()> {
    svc!(host, shell, |s| s.launch_app(&id))?.map_err(err)
}

#[tauri::command]
pub async fn shell_execute(path: String, args: Option<String>, verb: Option<String>) -> R<()> {
    arui_shell::shell_execute(&path, args.as_deref(), verb.as_deref()).map_err(err)
}

#[tauri::command]
pub fn shell_open_windows_start(host: State<Host>) -> R<()> {
    svc!(host, shell, |s| s.open_windows_start())?.map_err(err)
}

// ---------------------------------------------------------------- tray

#[tauri::command]
pub fn tray_list(host: State<Host>) -> Vec<TrayIcon> {
    svc!(host, tray, |t| t.icons()).unwrap_or_default()
}

#[tauri::command]
pub fn tray_action(host: State<Host>, id: String, action: TrayAction, x: i32, y: i32) -> R<()> {
    svc!(host, tray, |t| t.action(&id, action, x, y))?.map_err(err)
}

/// Not in protocol.ts yet: lets the tray widget report where icons are drawn (screen px).
#[tauri::command]
pub fn tray_set_icon_rects(host: State<Host>, rects: Vec<(String, Rect)>) -> R<()> {
    svc!(host, tray, |t| t.set_icon_rects(rects))
}

// ---------------------------------------------------------------- media / system

#[tauri::command]
pub fn media_list(host: State<Host>) -> Value {
    let (sessions, current): (Vec<MediaSession>, Option<String>) =
        svc!(host, media, |m| m.snapshot()).unwrap_or_default();
    json!({ "sessions": sessions, "current": current })
}

#[tauri::command]
pub async fn media_control(host: State<'_, Host>, id: Option<String>, action: MediaAction) -> R<()> {
    svc!(host, media, |m| m.control(id.as_deref(), action))?.map_err(err)
}

#[tauri::command]
pub fn volume_get(host: State<Host>) -> R<VolumeState> {
    svc!(host, system, |s| s.volume())
}
#[tauri::command]
pub fn volume_set(host: State<Host>, level: f32) -> R<()> {
    svc!(host, system, |s| s.set_volume(level))?.map_err(err)
}
#[tauri::command]
pub fn volume_mute(host: State<Host>, muted: bool) -> R<()> {
    svc!(host, system, |s| s.set_muted(muted))?.map_err(err)
}
#[tauri::command]
pub fn network_get(host: State<Host>) -> R<NetworkState> {
    svc!(host, system, |s| s.network())
}

#[tauri::command]
pub fn monitors_list(host: State<Host>) -> Vec<MonitorInfo> {
    host.monitors()
}

// ---------------------------------------------------------------- surfaces

#[tauri::command]
pub async fn surfaces_define(app: AppHandle, specs: Vec<SurfaceSpec>) -> R<()> {
    host::define_surfaces(&app, specs).await;
    Ok(())
}

#[tauri::command]
pub async fn surface_open(app: AppHandle, id: String, monitor: Option<String>, rect: Option<Rect>, params: Option<serde_json::Map<String, Value>>) -> R<()> {
    host::open_surface(&app, OpenArgs { id, monitor, rect, params }).await
}

#[tauri::command]
pub async fn surface_toggle(app: AppHandle, id: String, monitor: Option<String>, rect: Option<Rect>, params: Option<serde_json::Map<String, Value>>) -> R<()> {
    host::toggle_surface(&app, OpenArgs { id, monitor, rect, params }).await
}

#[tauri::command]
pub fn surface_close(app: AppHandle, id: String, monitor: Option<String>) {
    host::close_surface(&app, &id, monitor.as_deref());
}

#[tauri::command]
pub fn surface_resize(window: WebviewWindow, host: State<Host>, width: Option<f64>, height: Option<f64>, thickness: Option<f64>) -> R<()> {
    let scale = window.scale_factor().map_err(err)?;
    let (is_appbar, centered) = host
        .surfaces
        .lock()
        .windows
        .get(window.label())
        .map(|w| (w.spec.window.is_appbar(), w.spec.window.anchor == Some(crate::surfaces::Anchor::Rect)))
        .unwrap_or((false, false));
    if is_appbar {
        if let Some(t) = thickness.or(height) {
            let hwnd = win_hwnd(&window)?;
            let px = (t * scale).round() as i32;
            let r = svc!(host, shell, |s| s.update_appbar(hwnd, px))?.map_err(err)?;
            let _ = window.set_size(tauri::PhysicalSize::new(r.width() as u32, r.height() as u32));
            let _ = window.set_position(tauri::PhysicalPosition::new(r.left, r.top));
        }
        return Ok(());
    }
    let cur = window.inner_size().map_err(err)?;
    let w = width.map(|v| (v * scale).round() as u32).unwrap_or(cur.width);
    let h = height.map(|v| (v * scale).round() as u32).unwrap_or(cur.height);
    // Keep the bottom edge fixed so popups above the taskbar grow upwards. Popups anchored to
    // an element also keep their horizontal center, so they stay over that element.
    let pos = window.outer_position().map_err(err)?;
    let dy = h as i32 - cur.height as i32;
    let mut x = pos.x;
    if centered {
        x -= (w as i32 - cur.width as i32) / 2;
        if let Ok(Some(m)) = window.current_monitor() {
            let (mx, mw) = (m.position().x, m.size().width as i32);
            x = x.clamp(mx, (mx + mw - w as i32).max(mx));
        }
    }
    window.set_size(tauri::PhysicalSize::new(w, h)).map_err(err)?;
    window.set_position(tauri::PhysicalPosition::new(x, pos.y - dy)).map_err(err)
}

#[tauri::command]
pub fn surface_visible(window: WebviewWindow) -> bool {
    window.is_visible().unwrap_or(true)
}

#[tauri::command]
pub fn surface_params(window: WebviewWindow, host: State<Host>) -> Value {
    host.surfaces
        .lock()
        .windows
        .get(window.label())
        .map(|w| w.params.clone())
        .unwrap_or_else(|| json!({}))
}

// ---------------------------------------------------------------- mods & settings

#[tauri::command]
pub fn mods_list(host: State<Host>) -> Vec<mods::InstalledMod> {
    mods::scan(&crate::paths::mods_dir(), &host.settings)
}

#[tauri::command]
pub fn mod_set_enabled(app: AppHandle, host: State<Host>, id: String, enabled: bool) -> R<()> {
    mods::set_enabled(&host.settings, &id, enabled).map_err(err)?;
    let _ = app.emit("arui://settings-changed", json!({ "namespace": "core", "key": "enabledMods", "value": host.settings.get("core", "enabledMods") }));
    let _ = app.emit("arui://mods-changed", mods::scan(&crate::paths::mods_dir(), &host.settings));
    Ok(())
}

#[tauri::command]
pub async fn mods_open_folder() -> R<()> {
    let dir = crate::paths::mods_dir();
    std::fs::create_dir_all(&dir).map_err(err)?;
    arui_shell::shell_execute(&dir.to_string_lossy(), None, None).map_err(err)
}

#[tauri::command]
pub fn settings_get(host: State<Host>) -> Value {
    host.settings.all()
}

#[tauri::command]
pub fn settings_set(app: AppHandle, host: State<Host>, namespace: String, key: String, value: Value) -> R<()> {
    host.settings.set(&namespace, &key, value.clone()).map_err(err)?;
    let _ = app.emit("arui://settings-changed", json!({ "namespace": namespace, "key": key, "value": value }));
    Ok(())
}

// ---------------------------------------------------------------- app

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    app.state::<Host>().shutdown();
    app.exit(0);
}

#[tauri::command]
pub fn app_restart(app: AppHandle) {
    app.state::<Host>().shutdown();
    app.restart();
}
