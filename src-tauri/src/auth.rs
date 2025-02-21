use std::time::Duration;

use oauth2::{
    basic::{BasicClient, BasicTokenType},
    AuthUrl, ClientId, ClientSecret, CsrfToken, EmptyExtraTokenFields, PkceCodeChallenge,
    RedirectUrl, Scope, StandardTokenResponse, TokenUrl,
};
use tauri::{Listener, Url};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

use crate::constants::{AuthRedirectEventPayload, AUTH_REDIRECT_EVENT};

pub async fn get_token(
    app: &tauri::AppHandle,
) -> Result<StandardTokenResponse<EmptyExtraTokenFields, BasicTokenType>, Box<dyn std::error::Error>>
{
    let client = BasicClient::new(
        ClientId::new(std::env::var("GITHUB_CLIENT_ID").expect("GITHUB_CLIENT_ID must be set.")),
        Some(ClientSecret::new(
            std::env::var("GITHUB_CLIENT_SECRET").expect("GITHUB_CLIENT_SECRET must be set."),
        )),
        AuthUrl::new("https://github.com/login/oauth/authorize".to_string()).unwrap(),
        Some(TokenUrl::new("https://github.com/login/oauth/access_token".to_string()).unwrap()),
    )
    .set_redirect_uri(RedirectUrl::new("github-notifier://auth".to_string()).unwrap());

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("notifications".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    app.opener()
        .open_url(auth_url.as_str(), None::<&str>)
        .unwrap();

    let (code, state) = {
        let (tx, rx) = oneshot::channel();
        app.once(AUTH_REDIRECT_EVENT, |event| {
            if let Ok(AuthRedirectEventPayload { url }) = serde_json::from_str(event.payload()) {
                tx.send(url).unwrap()
            }
        });

        let request_line = tokio::time::timeout(Duration::from_secs(60 * 5), rx)
            .await
            .unwrap()
            .unwrap();
        let url = Url::parse(&request_line).unwrap();
        let code = url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, code)| oauth2::AuthorizationCode::new(code.into_owned()))
            .unwrap();
        let state = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, state)| oauth2::CsrfToken::new(state.into_owned()))
            .unwrap();

        (code, state)
    };

    assert!(csrf_token.secret() == state.secret());

    let token_response = client
        .exchange_code(code)
        .set_pkce_verifier(pkce_verifier)
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .unwrap();

    Ok(token_response)
}
