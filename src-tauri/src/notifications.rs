use std::time::Duration;

use futures::{stream, Stream};
use serde::Deserialize;
use tauri_plugin_http::reqwest;

#[derive(Deserialize, Debug)]
pub struct Subject {
    pub title: String,
    pub url: String,
    pub latest_comment_url: String,
    pub r#type: String,
}

#[derive(Deserialize, Debug)]
pub struct GithubResponse {
    pub id: String,
    pub reason: String,
    pub unread: bool,
    pub updated_at: String,
    pub subject: Subject,
}

async fn fetch_notifications(
    last_modified: Option<String>,
    token: &str,
) -> Result<(Option<Vec<GithubResponse>>, Option<u64>, Option<String>), reqwest::Error> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.append("X-GitHub-Api-Version", "2022-11-28".parse().unwrap());
    headers.append(
        reqwest::header::USER_AGENT,
        "Github Notifier".parse().unwrap(),
    );
    headers.append(
        reqwest::header::ACCEPT,
        "application/vnd.github+json".parse().unwrap(),
    );
    headers.append(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {}", token).parse().unwrap(),
    );
    if let Some(last_modified) = last_modified {
        headers.append(
            reqwest::header::IF_MODIFIED_SINCE,
            last_modified.parse().unwrap(),
        );
    }

    let response = reqwest::Client::new()
        .get("https://api.github.com/notifications")
        .headers(headers)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .expect("failed to send request");
    let interval_header = response
        .headers()
        .get("X-Poll-Interval")
        .and_then(|value| value.to_str().ok()?.parse().ok());
    let last_modified = response
        .headers()
        .get("Last-Modified")
        .and_then(|value| value.to_str().ok().map(|s| s.to_owned()));
    let notifications = match response.status() {
        reqwest::StatusCode::OK => Some(response.json::<Vec<GithubResponse>>().await?),
        reqwest::StatusCode::NOT_MODIFIED => None,
        code => {
            println!("Status code: {:?}", code);
            println!(
                "Failed to fetch notifications: {:?}",
                response.text().await?
            );
            None
        }
    };

    Ok((notifications, interval_header, last_modified))
}

pub fn notifications_stream(token: &str) -> impl Stream<Item = Option<Vec<GithubResponse>>> + '_ {
    stream::unfold(
        (tokio::time::interval(Duration::from_secs(60)), None),
        move |(mut interval, last_modified)| async move {
            interval.tick().await;

            match fetch_notifications(last_modified.clone(), token).await {
                Ok((new_notifications, new_interval_duration, new_last_modified)) => {
                    let last_modified_time = new_last_modified.or(last_modified);
                    let interval = match new_interval_duration {
                        Some(duration) if duration != interval.period().as_secs() => {
                            println!(
                                "Changing interval duration to {:?}",
                                Duration::from_secs(duration)
                            );
                            let mut new_interval =
                                tokio::time::interval(Duration::from_secs(duration));
                            new_interval.tick().await;

                            new_interval
                        }
                        _ => interval,
                    };

                    Some((new_notifications, (interval, last_modified_time)))
                }
                Err(e) => {
                    println!("{:?}", e);
                    None
                }
            }
        },
    )
}
