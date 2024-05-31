use std::time::Duration;

use base64::Engine;
use futures::{stream, Stream, TryFutureExt};
use serde::Deserialize;
use tauri_plugin_http::reqwest;
use url::Url;

#[derive(Deserialize, Debug)]
pub struct SomeGithubResponse {
    pub html_url: String,
}

#[derive(Deserialize, Debug)]
pub struct Subject {
    pub title: String,
    pub url: String,
    pub latest_comment_url: Option<String>,
    pub r#type: String,
}

#[derive(Deserialize, Debug)]
pub struct Owner {
    pub avatar_url: String,
}

#[derive(Deserialize, Debug)]
pub struct Repository {
    pub id: i32,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub owner: Owner,
}

#[derive(Deserialize, Debug)]
pub struct NotificationThread {
    pub id: String,
    pub repository: Repository,
    pub subject: Subject,
    pub reason: String,
    pub unread: bool,
    pub updated_at: Option<String>,
    pub last_read_at: Option<String>,
    pub url: String,
    pub subscription_url: String,
}

#[derive(Deserialize, Debug)]
pub struct User {
    pub id: i32,
    pub login: String,
}

pub struct GitHub {
    http_client: reqwest::Client,
    pub user: User,
}

impl GitHub {
    pub async fn new(token: String) -> Self {
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
            format!("Bearer {}", &token).parse().unwrap(),
        );

        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();
        let user = http_client
            .get("https://api.github.com/user")
            .send()
            .and_then(|response| response.json::<User>())
            .await
            .unwrap();

        Self { http_client, user }
    }

    async fn fetch_notifications(
        &self,
        last_modified: Option<String>,
    ) -> Result<(Option<Vec<NotificationThread>>, Option<u64>, Option<String>), reqwest::Error>
    {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(last_modified) = last_modified {
            headers.append(
                reqwest::header::IF_MODIFIED_SINCE,
                reqwest::header::HeaderValue::from_str(&last_modified).unwrap(),
            );
        }
        let response = self
            .http_client
            .get("https://api.github.com/notifications")
            .headers(headers)
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        let interval_header = response
            .headers()
            .get("X-Poll-Interval")
            .and_then(|value| value.to_str().ok()?.parse().ok());
        let last_modified = response
            .headers()
            .get("Last-Modified")
            .and_then(|value| value.to_str().ok().map(|s| s.to_owned()));
        let notifications = match response.status() {
            reqwest::StatusCode::OK => Some(response.json::<Vec<NotificationThread>>().await?),
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

    pub fn notifications_stream(&self) -> impl Stream<Item = Option<Vec<NotificationThread>>> + '_ {
        stream::unfold(
            (tokio::time::interval(Duration::from_secs(60)), None),
            move |(mut interval, last_modified)| async move {
                interval.tick().await;

                match self.fetch_notifications(last_modified.clone()).await {
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

    pub async fn generate_github_url(
        &self,
        notification_thread: &NotificationThread,
        user_id: i32,
    ) -> Option<url::Url> {
        let referrer_id = Self::generate_notification_referrer_id(&notification_thread.id, user_id);
        let base_url = match &notification_thread.subject {
            Subject {
                latest_comment_url: Some(url),
                ..
            } => self.fetch_html_url(url).await,
            Subject { url, .. } => self.fetch_html_url(url).await,
        };

        match base_url {
            Ok(url) => {
                Url::parse_with_params(&url, &[("notification_referrer_id", referrer_id)]).ok()
            }
            Err(_) => None,
        }
    }

    pub async fn fetch_html_url(&self, url: &str) -> Result<String, reqwest::Error> {
        self.http_client
            .get(url)
            .send()
            .await?
            .json::<SomeGithubResponse>()
            .await
            .map(|response| response.html_url)
    }

    pub fn generate_notification_referrer_id(notification_id: &str, user_id: i32) -> String {
        // https://github.com/sindresorhus/notifier-for-github/issues/268
        let referrer_id = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            [
                // MAGIC BYTES ✨
                vec![0x93, 0x00, 0xCE, 0x00, 0x2B, 0x69, 0x90, 0xB3],
                notification_id.into(),
                ":".into(),
                user_id.to_string().into(),
            ]
            .concat(),
        );

        format!("NT_{}", referrer_id)
    }
}
