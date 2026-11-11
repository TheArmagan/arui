// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod host;
mod mods;
mod paths;
mod schemes;
mod settings;
mod surfaces;

use tauri::{Emitter, Manager, RunEvent};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // If we panic, put explorer's taskbar back before dying (the guard covers hard kills).
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        arui_shell::emergency_restore(&[]);
        default_hook(info);
    }));

    let icons = arui_win::IconStore::new();
    let icon_store = icons.clone();
    let mods_dir = paths::mods_dir();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}))
        .manage(host::Host::new(icons))
        .register_uri_scheme_protocol("arui-icon", move |_ctx, req| schemes::icon(&icon_store, req.uri().path()))
        .register_uri_scheme_protocol("arui-mod", move |_ctx, req| schemes::mod_file(&mods_dir, req.uri().path()))
        .invoke_handler(tauri::generate_handler![
            commands::windows_list,
            commands::window_focus,
            commands::window_toggle,
            commands::window_minimize,
            commands::window_maximize,
            commands::window_restore,
            commands::window_close,
            commands::preview_show,
            commands::preview_hide,
            commands::apps_list,
            commands::app_launch,
            commands::shell_execute,
            commands::shell_open_windows_start,
            commands::tray_list,
            commands::tray_action,
            commands::tray_set_icon_rects,
            commands::media_list,
            commands::media_control,
            commands::volume_get,
            commands::volume_set,
            commands::volume_mute,
            commands::network_get,
            commands::monitors_list,
            commands::surfaces_define,
            commands::surface_open,
            commands::surface_toggle,
            commands::surface_close,
            commands::surface_resize,
            commands::surface_params,
            commands::surface_visible,
            commands::mods_list,
            commands::mod_set_enabled,
            commands::mods_open_folder,
            commands::settings_get,
            commands::settings_set,
            commands::app_quit,
            commands::app_restart,
        ])
        .setup(|app| {
            host::start_natives_async(app.handle());

            // Hot reload: tell every window when a mod's files or the mod list change.
            let handle = app.handle().clone();
            let watcher = mods::watch(paths::mods_dir(), move |change| match change {
                mods::ModChange::Files(id) => {
                    let _ = handle.emit("arui://mod-changed", serde_json::json!({ "id": id }));
                }
                mods::ModChange::List => {
                    let host = handle.state::<host::Host>();
                    let _ = handle.emit("arui://mods-changed", mods::scan(&paths::mods_dir(), &host.settings));
                }
            });
            match watcher {
                Ok(w) => *app.state::<host::Host>().watcher.lock() = Some(w),
                Err(e) => log::warn!("mods watcher failed: {e}"),
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building ARUI");

    app.run(|app, event| match event {
        // The host window is hidden; closing surface windows must not quit ARUI.
        RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        RunEvent::Exit => app.state::<host::Host>().shutdown(),
        _ => {}
    });
}
