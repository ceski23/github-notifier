use std::sync::{Arc, Mutex};

use std::path::MAIN_SEPARATOR as SEP;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

use crate::{github::NotificationThread, utils};

#[cfg(windows)]
pub async fn show_notification(
    thread: &NotificationThread,
    app_handle: AppHandle,
    url: String,
    github: &crate::github::GitHub,
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
        tauri_winrt_notification::Toast::POWERSHELL_APP_ID
    };

    let icon = Arc::new(Mutex::new(
        utils::download_icon(thread.repository.owner.avatar_url.as_str())
            .await
            .unwrap(),
    ));

    tauri_winrt_notification::Toast::new(app_id)
        .title(thread.subject.title.as_str())
        .text1(thread.repository.full_name.as_str())
        .icon(
            icon.lock().unwrap().path(),
            tauri_winrt_notification::IconCrop::Circular,
            thread.subject.title.as_str(),
        )
        .add_button("Mark as done", "done")
        .add_button("Unsubscribe", "unsubscribe")
        .on_activated({
            let icon = Arc::clone(&icon);
            let thread_id = thread.id.clone();
            let github = github.clone();

            move |action| {
                icon.lock().unwrap().clone().cleanup().unwrap();

                match action.as_deref() {
                    Some("done") => {
                        tauri::async_runtime::spawn({
                            let thread_id = thread_id.clone();
                            let github = github.clone();
                            async move {
                                github.mark_thread_as_done(&thread_id).await.unwrap();
                            }
                        });
                    }
                    Some("unsubscribe") => {
                        tauri::async_runtime::spawn({
                            let thread_id = thread_id.clone();
                            let github = github.clone();
                            async move {
                                github.delete_thread_subscription(&thread_id).await.unwrap();
                                github.mark_thread_as_done(&thread_id).await.unwrap();
                            }
                        });
                    }
                    _ => {
                        app_handle.shell().open(&url, None).unwrap();
                    }
                }

                Ok(())
            }
        })
        .on_dismissed({
            let icon = Arc::clone(&icon);

            move |_| {
                icon.lock().unwrap().clone().cleanup().unwrap();
                Ok(())
            }
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
    github: &crate::github::GitHub,
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
        .main_button(mac_notification_sys::MainButton::DropdownActions(
            "Dropdown",
            &["Mark as done", "Unsubscribe"],
        ))
        .content_image(icon.path().to_str().unwrap())
        .send()
        .unwrap();

    match (response) {
        mac_notification_sys::NotificationResponse::ActionButton(action_name) => {
            if action_name == "Mark as done" {
                println!("Clicked on Mark as done")
            } else if action_name == "Unsubscribe" {
                println!("Clicked on Unsubscribe")
            }
        }
        mac_notification_sys::NotificationResponse::Click => {
            let _ = app_handle.shell().open(&url, None);
        }
        _ => {}
    };

    Ok(())
}
