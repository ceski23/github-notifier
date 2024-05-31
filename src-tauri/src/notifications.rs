use std::sync::{Arc, Mutex};

use std::path::MAIN_SEPARATOR as SEP;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;
use tauri_winrt_notification::Toast;

use crate::{github::NotificationThread, utils};

#[cfg(windows)]
pub async fn show_notification(
    thread: &NotificationThread,
    app_handle: AppHandle,
    url: String,
) -> anyhow::Result<()> {
    let exe = tauri::utils::platform::current_exe()?;
    let exe_dir = exe.parent().expect("failed to get exe directory");
    let curr_dir = exe_dir.display().to_string();
    // set the notification's System.AppUserModel.ID only when running the installed app
    let app_id = if !(curr_dir.ends_with(format!("{SEP}target{SEP}debug").as_str())
        || curr_dir.ends_with(format!("{SEP}target{SEP}release").as_str()))
    {
        app_handle.config().identifier.as_str()
    } else {
        Toast::POWERSHELL_APP_ID
    };

    let icon = Arc::new(Mutex::new(
        utils::download_icon(thread.repository.owner.avatar_url.as_str())
            .await
            .unwrap(),
    ));
    // FIXME: this probably isn't the best way to handle this
    let activated_closure_icon = Arc::clone(&icon);
    let dismissed_closure_icon = Arc::clone(&icon);
    let icon_lock = icon.lock().unwrap();

    Toast::new(app_id)
        .title(thread.subject.title.as_str())
        .text1(thread.repository.full_name.as_str())
        .icon(
            icon_lock.path(),
            tauri_winrt_notification::IconCrop::Circular,
            thread.subject.title.as_str(),
        )
        .add_button("Mark as done", "done")
        .add_button("Unsubscribe", "unsubscribe")
        .on_activated(move |_| {
            let _ = app_handle.shell().open(&url, None);

            activated_closure_icon
                .lock()
                .unwrap()
                .clone()
                .cleanup()
                .unwrap();

            Ok(())
        })
        .on_dismissed(move |_| {
            dismissed_closure_icon
                .lock()
                .unwrap()
                .clone()
                .cleanup()
                .unwrap();

            Ok(())
        })
        .show()
        .unwrap();

    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn show_notification(
    thread: &NotificationThread,
    app_handle: AppHandle,
    url: String,
) -> anyhow::Result<()> {
    let app_id = if tauri::is_dev() {
        "com.apple.Terminal"
    } else {
        app_handle.config().identifier.as_str()
    };
    let icon = utils::download_icon(thread.repository.owner.avatar_url.as_str())
        .await
        .unwrap();

    mac_notification_sys::set_application(&app_id).unwrap();
    let response = mac_notification_sys::Notification::default()
        .title(thread.subject.title.as_str())
        .message(thread.repository.full_name.as_str())
        .main_button(MainButton::DropdownActions(
            "Dropdown",
            &["Mark as done", "Unsubscribe"],
        ))
        .content_image(icon.path())
        .send()
        .unwrap();

    match (response) {
        NotificationResponse::ActionButton(action_name) => {
            if action_name == "Mark as done" {
                println!("Clicked on Mark as done")
            } else if action_name == "Unsubscribe" {
                println!("Clicked on Unsubscribe")
            }
        }
        NotificationResponse::Click => {
            let _ = app_handle.shell().open(&url, None);
        }
        _ => {}
    };

    Ok(())
}