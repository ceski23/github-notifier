// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures::StreamExt;
use oauth2::TokenResponse;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_notification::NotificationExt;

mod auth;
mod notifications;

fn main() {
    dotenv::dotenv().ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .setup(setup)
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let autostart_manager = app.autolaunch();
    let _ = autostart_manager.enable();
    println!(
        "registered for autostart? {}",
        autostart_manager.is_enabled().unwrap()
    );
    let app_handle = app.handle().clone();
    let app_handle2 = app.handle().clone();

    match app.notification().permission_state().unwrap() {
        tauri_plugin_notification::PermissionState::Denied
        | tauri_plugin_notification::PermissionState::Unknown => {
            app.notification().request_permission().unwrap();
        }
        tauri_plugin_notification::PermissionState::Granted => {}
    }

    app.listen("deep-link://new-url", move |_| {
        let url = app_handle2
            .deep_link()
            .get_current()
            .unwrap()
            .unwrap()
            .first()
            .unwrap()
            .to_string();
        println!("Received deep link: {}", url);
        app_handle2
            .notification()
            .builder()
            .title("lol")
            .body(url)
            .show()
            .unwrap();
    });

    let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let auth_item = MenuItemBuilder::with_id("auth", "Authenticate").build(app)?;
    let mut menu_builder = MenuBuilder::new(app);

    let token_entry = keyring::Entry::new("github-notifier", "user").unwrap();
    let token = token_entry.get_password();

    if token.is_err() {
        menu_builder = menu_builder.item(&auth_item);
    }

    let menu = menu_builder.item(&quit_item).build()?;

    TrayIconBuilder::new()
        .title("2")
        .tooltip(app.package_info().name.clone())
        .icon(app.default_window_icon().unwrap().to_owned())
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "quit" => {
                app.exit(0);
            }
            "auth" => {
                let app_handle = app.clone();

                tauri::async_runtime::spawn(async move {
                    let token_response = auth::get_token(&app_handle).await.unwrap();
                    let token_entry = keyring::Entry::new("github-notifier", "user").unwrap();
                    token_entry
                        .set_password(token_response.access_token().secret())
                        .unwrap();

                    // TODO: remove menu item
                    // menu.remove(&auth_item).unwrap();

                    start_monitoring_notifications(
                        app_handle,
                        token_response.access_token().secret().to_string(),
                    );
                });
            }
            _ => (),
        })
        .build(app)?;

    if let Ok(token) = token {
        start_monitoring_notifications(app_handle, token)
    }

    Ok(())
}

fn start_monitoring_notifications(app_handle: tauri::AppHandle, token: String) {
    tauri::async_runtime::spawn(async move {
        notifications::notifications_stream(token.as_str())
            .for_each(|new_notifications| async {
                println!("New notifications: {:?}", new_notifications);
                match new_notifications {
                    Some(notifications) if notifications.len() < 5 => {
                        for notification in notifications.iter() {
                            app_handle
                                .notification()
                                .builder()
                                .title(notification.subject.title.as_str())
                                .body(notification.subject.url.as_str())
                                .show()
                                .unwrap();
                        }
                    }
                    Some(notifications) => {
                        app_handle
                            .notification()
                            .builder()
                            .title("New notifications!")
                            .body(format!(
                                "You have {} new notifications",
                                notifications.len()
                            ))
                            .show()
                            .unwrap();
                    }
                    _ => (),
                }
            })
            .await;
    });
}
