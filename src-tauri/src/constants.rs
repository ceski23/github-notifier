pub const AUTH_REDIRECT_EVENT: &str = "auth_redirect";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthRedirectEventPayload {
    pub url: String,
}
