// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use constants::{AuthRedirectEventPayload, AUTH_REDIRECT_EVENT};
use futures::StreamExt;
use oauth2::TokenResponse;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Wry,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_shell::ShellExt;

mod auth;
mod constants;
mod github;
mod notifications;
mod utils;

fn main() {
    dotenv::dotenv().ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            |app, argv, _: String| match argv.get(1) {
                Some(url) if url.starts_with("github-notifier://auth") => {
                    app.emit(
                        AUTH_REDIRECT_EVENT,
                        AuthRedirectEventPayload {
                            url: url.to_string(),
                        },
                    )
                    .unwrap();
                }
                _ => {}
            },
        ))
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

    // TODO: code below is shit, refactor it
    let app_handle = app.handle().clone();
    let app_handle2 = app.handle().clone();

    match app.notification().permission_state().unwrap() {
        tauri_plugin_notification::PermissionState::Denied
        | tauri_plugin_notification::PermissionState::Unknown => {
            app.notification().request_permission().unwrap();
        }
        tauri_plugin_notification::PermissionState::Granted => {}
    }

    match app.deep_link().is_registered("github-notifier") {
        Ok(true) => {}
        _ => {
            app.deep_link().register("github-notifier").unwrap();
        }
    }

    app.listen("deep-link://new-url", move |event| {
        let url = app_handle2
            .deep_link()
            .get_current()
            .unwrap()
            .unwrap()
            .first()
            .unwrap()
            .to_string();
        println!("URL: {}", url);
        println!("EVENT: {}", event.payload());
        app_handle2
            .notification()
            .builder()
            .body(event.payload())
            .show()
            .unwrap();
        app_handle2
            .notification()
            .builder()
            .body(&url)
            .show()
            .unwrap();
        app_handle2.emit(AUTH_REDIRECT_EVENT, url).unwrap();
    });

    let token_entry = keyring::Entry::new("github-notifier", "user").unwrap();
    let token = token_entry.get_password().ok();

    setup_tray(&app_handle, token.is_some())?;

    if let Some(token) = token {
        start_monitoring_notifications(app_handle.clone(), token)
    }

    Ok(())
}

fn start_monitoring_notifications(app_handle: tauri::AppHandle, token: String) {
    tauri::async_runtime::spawn(async move {
        let github = github::GitHub::new(token).await;

        // TODO: handle errors
        github
            .notifications_stream()
            .for_each(|threads| async {
                match threads {
                    Some(threads) => {
                        app_handle
                            .tray_by_id("tray")
                            .unwrap()
                            .set_title(Some(threads.len().to_string()))
                            .unwrap();

                        if threads.len() < 5 {
                            for thread in threads.iter() {
                                let url = github
                                    .generate_github_url(thread, github.user.id)
                                    .await
                                    .map_or(
                                        String::from("https://github.com/notifications"),
                                        |url| url.into(),
                                    );

                                notifications::show_notification(thread, app_handle.clone(), url)
                                    .await
                                    .unwrap();
                            }
                        } else {
                            app_handle
                                .notification()
                                .builder()
                                .title("New notifications!")
                                .body(format!("You have {} new notifications", threads.len()))
                                .show()
                                .unwrap();
                        }
                    }
                    None => {
                        app_handle
                            .tray_by_id("tray")
                            .unwrap()
                            .set_title(None::<String>)
                            .unwrap();
                    }
                }
            })
            .await;
    });
}

fn create_tray_menu(
    app: &AppHandle,
    is_authorized: bool,
) -> Result<tauri::menu::Menu<Wry>, Box<dyn std::error::Error>> {
    let mut menu_builder = MenuBuilder::new(app);

    if !is_authorized {
        menu_builder =
            menu_builder.item(&MenuItemBuilder::with_id("auth", "Authenticate").build(app)?);
    }

    let menu = menu_builder
        .item(&MenuItemBuilder::with_id("notifications", "Open notifications").build(app)?)
        .item(&PredefinedMenuItem::quit(app, Some("Quit"))?)
        .build()?;

    Ok(menu)
}

fn setup_tray(app: &AppHandle, is_authorized: bool) -> Result<(), Box<dyn std::error::Error>> {
    TrayIconBuilder::with_id("tray")
        .tooltip(app.package_info().name.clone())
        .icon(app.default_window_icon().unwrap().to_owned())
        .menu(&create_tray_menu(app, is_authorized)?)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "auth" => {
                let app_handle = app.clone();

                tauri::async_runtime::spawn(async move {
                    let token_response = auth::get_token(&app_handle).await.unwrap();
                    let token_entry = keyring::Entry::new("github-notifier", "user").unwrap();
                    token_entry
                        .set_password(token_response.access_token().secret())
                        .unwrap();

                    if let Some(tray) = app_handle.tray_by_id("tray") {
                        tray.set_menu(create_tray_menu(&app_handle, true).ok())
                            .unwrap();
                    }

                    start_monitoring_notifications(
                        app_handle,
                        token_response.access_token().secret().to_string(),
                    );
                });
            }
            "notifications" => {
                app.shell()
                    .open("https://github.com/notifications", None)
                    .unwrap();
            }
            _ => (),
        })
        .build(app)?;

    Ok(())
}
