// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::time::Duration;

use serde::Deserialize;
use tauri_plugin_http::reqwest;
use tauri_plugin_notification::NotificationExt;

fn main() {
    dotenv::dotenv().ok();

    println!("Hello, world!");
    tauri::Builder::default()
        .setup(setup)
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    println!("Start setup");
    let handle = app.handle().clone();

    if let Err(e) = handle
        .notification()
        .builder()
        .body("body")
        .title("title")
        .show()
    {
        println!("failed to show notification: {:?}", e);
    }

    if let Err(e) = app
        .notification()
        .builder()
        .body("body")
        .title("title")
        .show()
    {
        println!("failed to show notification app: {:?}", e);
    }

    tauri::async_runtime::spawn(async move {
        let result = get_notifications().await;

        match result {
            Ok(notifications) => {
                println!("{:?}", notifications.len());
                let res = handle.notification().request_permission().unwrap();
                println!("{:?}", res);

                handle
                    .notification()
                    .builder()
                    .title(notifications.len().to_string())
                    .show()
                    .unwrap();
            }
            Err(e) => {
                println!("{:?}", e);
            }
        }
    });

    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            println!("This line is printed every 5 seconds");
        }
    });
    Ok(())
}

#[derive(Deserialize, Debug)]
struct GithubResponse {
    id: String,
    reason: String,
    unread: bool,
    updated_at: String,
}

async fn get_notifications() -> Result<Vec<GithubResponse>, reqwest::Error> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.github.com/notifications")
        .header(reqwest::header::USER_AGENT, "Github Notifier")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(
            reqwest::header::AUTHORIZATION,
            format!(
                "Bearer {}",
                std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN must be set.")
            ),
        )
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .expect("failed to send request");

    response.json::<Vec<GithubResponse>>().await
}
