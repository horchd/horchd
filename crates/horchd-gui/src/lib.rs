//! horchd-gui: Tauri 2 tray + control panel.
//!
//! All daemon interaction goes through `dbus_client`, which constructs a
//! [`horchd_core::DaemonProxy`] and exposes async helpers to the Tauri
//! command layer. The frontend (whatever stack — vanilla HTML during the
//! scaffold phase, SvelteKit later) calls these via `tauri.invoke()`.

mod commands;
mod dbus_client;
mod tray;

use tracing_subscriber::EnvFilter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            tray::install(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_wakewords,
            commands::get_status,
            commands::set_threshold,
            commands::set_enabled,
            commands::set_cooldown,
            commands::add_wakeword,
            commands::remove_wakeword,
            commands::reload,
        ])
        .run(tauri::generate_context!())
        .expect("running horchd-gui");
}
