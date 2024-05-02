use oauth2::{
    basic::{BasicClient, BasicTokenType},
    AuthUrl, ClientId, ClientSecret, CsrfToken, EmptyExtraTokenFields, PkceCodeChallenge,
    RedirectUrl, Scope, StandardTokenResponse, TokenUrl,
};
use tauri::Url;
use tauri_plugin_shell::ShellExt;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

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
    .set_redirect_uri(RedirectUrl::new("http://localhost:12345/auth".to_string()).unwrap());

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("notifications".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    app.shell().open(auth_url.as_str(), None).unwrap();

    let (code, state) = {
        let listener = tokio::net::TcpListener::bind("localhost:12345")
            .await
            .unwrap();
        // TODO: add timeout
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).await.unwrap();

        let redirect_url = request_line.split_whitespace().nth(1).unwrap();
        let url = Url::parse(&("http://localhost".to_string() + redirect_url)).unwrap();

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

        let message =
            "<html><body>Go back to the application.<script>window.close();</script></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{}",
            message.len(),
            message
        );
        stream.write_all(response.as_bytes()).await.unwrap();

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
