//! horchd-gui: Tauri 2 tray + control panel.
//!
//! All daemon interaction goes through `dbus_client`, which constructs a
//! [`horchd_client::DaemonProxy`] and exposes async helpers to the Tauri
//! command layer. The frontend (vanilla HTML during the scaffold phase,
//! SvelteKit once that migration lands) calls these via `tauri.invoke()`
//! and listens to the `horchd://detected` event for live fires.

mod commands;
mod dbus_client;
mod events;
mod tray;

use tracing_subscriber::EnvFilter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    apply_wayland_workarounds();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            commands::register_state(app);
            tray::install(app)?;
            events::spawn(app.handle().clone());
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
            commands::import_wakeword,
            commands::models_dir,
            commands::list_input_devices,
            commands::set_input_device,
            commands::training_dir,
            commands::save_training_sample,
            commands::save_word_meta,
            commands::list_training_samples,
            commands::list_training_words,
            commands::delete_training_sample,
            commands::read_training_sample,
            commands::train_wakeword,
            commands::training_env_status,
            commands::setup_training_env,
            commands::fetch_negatives,
            commands::cancel_setup,
            commands::cancel_training,
        ])
        .run(tauri::generate_context!())
        .expect("running horchd-gui");
}

/// Work around `Gdk-Message: Error 71 (Protocol error) dispatching to
/// Wayland display` and similar webkit2gtk-on-Wayland breakage that
/// shows up on NVIDIA, mixed-DPI, and several Hyprland/KDE setups.
/// Both env vars are documented webkit2gtk knobs and are no-ops on
/// systems that don't need them. Already-set values from the user's
/// shell win — we only seed the defaults.
fn apply_wayland_workarounds() {
    let knobs = [
        ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
        ("WEBKIT_DISABLE_COMPOSITING_MODE", "1"),
    ];
    // SAFETY: called exactly once before tracing and before any tokio /
    // tauri thread is spawned, so no observer can be racing the env map.
    unsafe {
        for (k, v) in knobs {
            if std::env::var_os(k).is_none() {
                std::env::set_var(k, v);
            }
        }
    }
}
