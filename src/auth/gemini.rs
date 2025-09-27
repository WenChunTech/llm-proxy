use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use oauth2::basic::{BasicClient, BasicErrorResponseType, BasicTokenType};
use oauth2::{
    AuthUrl, Client, ClientId, ClientSecret, EmptyExtraTokenFields, EndpointNotSet, EndpointSet,
    RedirectUrl, RefreshToken, RevocationErrorResponseType, StandardErrorResponse,
    StandardRevocableToken, StandardTokenIntrospectionResponse, StandardTokenResponse,
    TokenResponse, TokenUrl,
};
use oauth2::{DeviceAuthorizationUrl, reqwest};
use tracing::{info, instrument};

use crate::config;

const OAUTH_CLIENT_ID: &str =
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
const OAUTH_CLIENT_SECRET: &str = "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";
const OAUTH_REDIRECT_URL: &str = "http://localhost:8085/oauth2callback";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DEVICE_AUTH_URL: &str = "https://oauth2.googleapis.com/device/code";

type GeminiClient = Client<
    StandardErrorResponse<BasicErrorResponseType>,
    StandardTokenResponse<EmptyExtraTokenFields, BasicTokenType>,
    StandardTokenIntrospectionResponse<EmptyExtraTokenFields, BasicTokenType>,
    StandardRevocableToken,
    StandardErrorResponse<RevocationErrorResponseType>,
    EndpointSet,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
>;
pub static GEMINI_CLIENT: LazyLock<GeminiClient> = LazyLock::new(|| {
    BasicClient::new(ClientId::new(OAUTH_CLIENT_ID.to_string()))
        .set_client_secret(ClientSecret::new(OAUTH_CLIENT_SECRET.to_string()))
        .set_auth_uri(AuthUrl::new(AUTH_URL.to_string()).expect("AuthUrl should build"))
        .set_token_uri(TokenUrl::new(TOKEN_URL.to_string()).expect("TokenUrl should build"))
        .set_device_authorization_url(
            DeviceAuthorizationUrl::new(DEVICE_AUTH_URL.to_string())
                .expect("DeviceAuthUrl should build"),
        )
        .set_redirect_uri(
            RedirectUrl::new(OAUTH_REDIRECT_URL.to_string()).expect("RedirectUrl should build"),
        )
});

pub fn client() -> &'static GeminiClient {
    &GEMINI_CLIENT
}

#[instrument]
pub async fn refresh_token(
    refresh_token: &str,
) -> Result<(
    StandardTokenResponse<EmptyExtraTokenFields, BasicTokenType>,
    u64,
)> {
    let http_client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Client should build");

    let refresh_token = RefreshToken::new(refresh_token.to_string());
    let token_result = client()
        .exchange_refresh_token(&refresh_token)
        .add_extra_param("client_id", OAUTH_CLIENT_ID)
        .add_extra_param("client_secret", OAUTH_CLIENT_SECRET)
        .request_async(&http_client)
        .await?;

    let expires_in = token_result
        .expires_in()
        .unwrap_or(Duration::from_secs(3599));
    let expiry = SystemTime::now() + expires_in - Duration::from_secs(60);

    Ok((
        token_result,
        expiry.duration_since(UNIX_EPOCH).unwrap().as_secs(),
    ))
}

fn is_token_expired(expire_at: u64) -> bool {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(3599))
        .as_secs()
        >= expire_at
}

#[instrument]
pub async fn token(config: &mut config::Config) -> Result<config::Token> {
    let token = config
        .gemini
        .clone()
        .unwrap()
        .first()
        .unwrap()
        .token
        .clone();
    if is_token_expired(token.expire_at) {
        info!("Token expired, refreshing...");
        let (new_token, expire_at) = refresh_token(&token.refresh_token).await?;
        let refresh_token = new_token
            .refresh_token()
            .map(|v| v.secret().to_string())
            .unwrap_or(token.refresh_token);
        let token_type = match new_token.token_type() {
            BasicTokenType::Bearer => "Bearer".to_string(),
            BasicTokenType::Mac => "Mac".to_string(),
            _ => "".to_string(),
        };
        let token = config::Token {
            access_token: new_token.access_token().secret().to_string(),
            refresh_token,
            expires_in: new_token.expires_in().unwrap().as_secs(),
            expire_at,
            token_type,
        };
        config.gemini.as_mut().unwrap().first_mut().unwrap().token = token.clone();
        config::save_config(config);
        info!("Token refreshed");
        Ok(token)
    } else {
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::*;

    #[tokio::test]
    async fn test_is_token_expired() {
        let mut config = config::load_config();
        token(&mut config).await.unwrap();
    }

    #[test]
    fn test_duration() {
        let duration = SystemTime::now();
        // convert SystemTime to timestamp
        let duration = duration.duration_since(UNIX_EPOCH).unwrap();
        let s = serde_json::to_string(&duration);
        match s {
            Ok(s) => {
                println!("{}", s);
            }
            Err(e) => {
                panic!("{}", e);
            }
        }
    }
}
