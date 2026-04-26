//! System tray icon + minimal menu. Left-click toggles the main window;
//! right-click opens "Open / Reload / Open Lyna trainer / Quit".

use tauri::{
    App, Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub fn install(app: &mut App) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "Open control panel").build(app)?;
    let reload = MenuItemBuilder::with_id("reload", "Reload config").build(app)?;
    let lyna = MenuItemBuilder::with_id("lyna", "Open Lyna trainer").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit horchd-gui").build(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[&open, &reload, &lyna, &quit])
        .build()?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "reload" => {
                tauri::async_runtime::spawn(async {
                    if let Err(err) = crate::commands::reload().await {
                        tracing::warn!(?err, "reload failed");
                    }
                });
            }
            "lyna" => {
                let _ = tauri_plugin_opener::open_url("http://localhost:5173", None::<&str>);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn show_main<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}
