use base64::Engine;
use serde_json::Value;

use crate::{
    config::{AuthEnabled, CodexAuth, GrokAuth, OneOrMany},
    error::ProxyError,
    util::{append_url_path, auth_string, set_auth_i64, set_auth_string},
};

pub(crate) const CODEX_USER_AGENT: &str =
    "codex-tui/0.135.0 (Mac OS 26.5.0; arm64) iTerm.app/3.6.10 (codex-tui; 0.135.0)";
pub(crate) const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub(crate) const CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub(crate) const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub(crate) const DEFAULT_GROK_BASE_URL: &str = "https://api.x.ai/v1";
pub(crate) const DEFAULT_GROK_TOKEN_ENDPOINT: &str = "https://auth.x.ai/oauth/token";
pub(crate) const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";

#[derive(Debug)]
pub struct AccessToken {
    pub token: String,
    pub refreshed: bool,
}

pub fn responses_endpoint(base_url: &str) -> Result<String, ProxyError> {
    let mut url = reqwest::Url::parse(base_url.trim())
        .map_err(|_| ProxyError::InvalidRequest("invalid base_url".to_string()))?;
    let pathname = url.path().trim_end_matches('/').to_string();

    if pathname.ends_with("/responses") {
        url.set_path(if pathname.is_empty() {
            "/responses"
        } else {
            &pathname
        });
    } else {
        url.set_path(&append_url_path(&pathname, "responses"));
    }

    Ok(url.to_string())
}

pub fn codex_oauth_base_url(provider_base_url: Option<&str>, auth: &CodexAuth) -> String {
    provider_base_url
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            auth.base_url
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or(DEFAULT_CODEX_BASE_URL)
        .trim_end_matches('/')
        .to_string()
}

pub fn grok_oauth_base_url(provider_base_url: Option<&str>, auth: &GrokAuth) -> String {
    provider_base_url
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            auth.base_url
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or(DEFAULT_GROK_BASE_URL)
        .trim_end_matches('/')
        .to_string()
}

pub fn select_codex_auth(
    auth: &OneOrMany<CodexAuth>,
    start_index: Option<usize>,
    target_attempt: usize,
) -> Result<(usize, CodexAuth), ProxyError> {
    select_auth(auth, start_index, target_attempt, "Codex")
}

pub fn select_grok_auth(
    auth: &OneOrMany<GrokAuth>,
    start_index: Option<usize>,
    target_attempt: usize,
) -> Result<(usize, GrokAuth), ProxyError> {
    select_auth(auth, start_index, target_attempt, "Grok")
}

/// Credential selected for Codex/Grok: either provider api_key or one auth entry.
#[derive(Debug, Clone)]
pub enum SelectedCredential<T> {
    ApiKey { token: String },
    Auth { index: usize, auth: T },
}

/// Number of tryable credentials (enabled auth entries + optional api_key).
pub fn oauth_credential_count<T: AuthEnabled>(api_key: &str, auth: &OneOrMany<T>) -> usize {
    let auth_count = auth.enabled_items_with_indices().len();
    let has_api_key = !api_key.trim().is_empty();
    auth_count + usize::from(has_api_key)
}

/// Select among api_key (if set) and enabled auth entries.
/// Order: api_key first, then enabled auths in config order.
/// `start_index` is the last successful auth array index (not including api_key).
pub fn select_oauth_credential<T>(
    api_key: &str,
    auth: &OneOrMany<T>,
    start_index: Option<usize>,
    target_attempt: usize,
    provider_name: &str,
) -> Result<SelectedCredential<T>, ProxyError>
where
    T: AuthEnabled + Clone,
{
    enum CredRef<'a, T> {
        ApiKey,
        Auth(usize, &'a T),
    }

    let api_key = api_key.trim();
    let accounts = auth.enabled_items_with_indices();
    let mut credentials: Vec<CredRef<'_, T>> = Vec::with_capacity(accounts.len() + 1);
    if !api_key.is_empty() {
        credentials.push(CredRef::ApiKey);
    }
    for (index, item) in accounts {
        credentials.push(CredRef::Auth(index, item));
    }
    if credentials.is_empty() {
        return Err(ProxyError::Config(format!(
            "No enabled {provider_name} credentials available (configure api_key or auth)"
        )));
    }

    let start = start_index
        .and_then(|index| {
            credentials.iter().position(|candidate| {
                matches!(candidate, CredRef::Auth(auth_index, _) if *auth_index == index)
            })
        })
        .unwrap_or(0);
    let offset = target_attempt.saturating_sub(1);
    let selected = &credentials[(start + offset) % credentials.len()];
    match selected {
        CredRef::ApiKey => Ok(SelectedCredential::ApiKey {
            token: api_key.to_string(),
        }),
        CredRef::Auth(index, item) => Ok(SelectedCredential::Auth {
            index: *index,
            auth: (*item).clone(),
        }),
    }
}

pub async fn codex_access_token(
    client: &reqwest::Client,
    auth: &mut CodexAuth,
) -> Result<AccessToken, ProxyError> {
    if let Some(token) = auth.access_token.as_ref().filter(|token| !token.is_empty()) {
        return Ok(AccessToken {
            token: token.clone(),
            refreshed: false,
        });
    }
    let token = refresh_codex_token(client, auth).await?;
    Ok(AccessToken {
        token,
        refreshed: true,
    })
}

pub async fn grok_access_token(
    client: &reqwest::Client,
    auth: &mut GrokAuth,
) -> Result<AccessToken, ProxyError> {
    if let Some(token) = auth.access_token.as_ref().filter(|token| !token.is_empty()) {
        return Ok(AccessToken {
            token: token.clone(),
            refreshed: false,
        });
    }
    let token = refresh_grok_token(client, auth).await?;
    Ok(AccessToken {
        token,
        refreshed: true,
    })
}

pub(crate) fn should_refresh_value(
    provider_type: crate::provider::types::ProviderType,
    auth: &Value,
) -> bool {
    if auth_string(auth, "refresh_token").is_none() {
        return false;
    }
    if auth_string(auth, "access_token").is_none() {
        return true;
    }
    let Some(expiry_date) = auth.get("expiry_date").and_then(Value::as_i64) else {
        return false;
    };
    let skew_ms = match provider_type {
        crate::provider::types::ProviderType::Codex => 60_000,
        crate::provider::types::ProviderType::Grok => 300_000,
        _ => 0,
    };
    current_millis() >= expiry_date - skew_ms
}

pub(crate) async fn refresh_codex_auth_value(
    client: &reqwest::Client,
    auth: &mut Value,
) -> Result<(), ProxyError> {
    let refresh_token = auth_string(auth, "refresh_token")
        .ok_or_else(|| ProxyError::Config("refresh_token is missing".to_string()))?;
    let token_data = refresh_codex_token_data(client, &refresh_token).await?;
    update_auth_value_from_token_response(auth, &token_data)?;
    copy_codex_claims_to_value(auth);
    Ok(())
}

pub(crate) async fn refresh_grok_auth_value(
    client: &reqwest::Client,
    auth: &mut Value,
) -> Result<(), ProxyError> {
    let refresh_token = auth_string(auth, "refresh_token")
        .ok_or_else(|| ProxyError::Config("refresh_token is missing".to_string()))?;
    let endpoint = auth
        .get("token_endpoint")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(DEFAULT_GROK_TOKEN_ENDPOINT);
    let token_data = refresh_grok_token_data(client, endpoint, &refresh_token).await?;
    update_auth_value_from_token_response(auth, &token_data)?;
    Ok(())
}

pub fn current_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn select_auth<T>(
    auth: &OneOrMany<T>,
    start_index: Option<usize>,
    target_attempt: usize,
    provider_name: &str,
) -> Result<(usize, T), ProxyError>
where
    T: AuthEnabled + Clone,
{
    let accounts = auth.enabled_items_with_indices();
    if accounts.is_empty() {
        return Err(ProxyError::Config(format!(
            "No enabled {provider_name} accounts available"
        )));
    }
    let start = start_index
        .and_then(|index| {
            accounts
                .iter()
                .position(|(candidate, _)| *candidate == index)
        })
        .unwrap_or_default();
    let offset = target_attempt.saturating_sub(1);
    let idx = (start + offset) % accounts.len();
    let (auth_index, auth) = accounts[idx];
    Ok((auth_index, (*auth).clone()))
}

async fn refresh_codex_token(
    client: &reqwest::Client,
    auth: &mut CodexAuth,
) -> Result<String, ProxyError> {
    let Some(refresh_token) = auth
        .refresh_token
        .as_deref()
        .filter(|token| !token.is_empty())
    else {
        return Err(ProxyError::Config(
            "Codex access_token is missing and refresh_token is not configured".to_string(),
        ));
    };

    let token_data = refresh_codex_token_data(client, refresh_token).await?;
    update_codex_auth_from_token_response(auth, &token_data)
}

async fn refresh_grok_token(
    client: &reqwest::Client,
    auth: &mut GrokAuth,
) -> Result<String, ProxyError> {
    let Some(refresh_token) = auth
        .refresh_token
        .as_deref()
        .filter(|token| !token.is_empty())
    else {
        return Err(ProxyError::Config(
            "Grok access_token is missing and refresh_token is not configured".to_string(),
        ));
    };

    let endpoint = auth
        .token_endpoint
        .as_deref()
        .filter(|endpoint| !endpoint.trim().is_empty())
        .unwrap_or(DEFAULT_GROK_TOKEN_ENDPOINT);
    let token_data = refresh_grok_token_data(client, endpoint, refresh_token).await?;
    update_grok_auth_from_token_response(auth, &token_data)
}

async fn refresh_codex_token_data(
    client: &reqwest::Client,
    refresh_token: &str,
) -> Result<Value, ProxyError> {
    let form = [
        ("client_id", CODEX_CLIENT_ID),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("scope", "openid email profile"),
    ];
    let response = client
        .post(CODEX_TOKEN_URL)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .form(&form)
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ProxyError::Upstream(format!(
            "Codex token refresh failed: {status} {body}"
        )));
    }

    Ok(response.json().await?)
}

async fn refresh_grok_token_data(
    client: &reqwest::Client,
    endpoint: &str,
    refresh_token: &str,
) -> Result<Value, ProxyError> {
    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", XAI_CLIENT_ID),
        ("refresh_token", refresh_token),
    ];
    let response = client
        .post(endpoint)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .form(&form)
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ProxyError::Upstream(format!(
            "Grok token refresh failed: {status} {body}"
        )));
    }

    Ok(response.json().await?)
}

fn update_codex_auth_from_token_response(
    auth: &mut CodexAuth,
    token_data: &Value,
) -> Result<String, ProxyError> {
    let access_token = token_access_token(token_data, "Codex")?;
    let expires_in = token_data
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3600);
    auth.access_token = Some(access_token.clone());
    auth.refresh_token = token_data
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| auth.refresh_token.clone());
    auth.id_token = token_data
        .get("id_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| auth.id_token.clone());
    auth.expiry_date = Some(current_millis() + expires_in * 1000);
    copy_codex_claims_to_auth(auth);
    Ok(access_token)
}

fn update_grok_auth_from_token_response(
    auth: &mut GrokAuth,
    token_data: &Value,
) -> Result<String, ProxyError> {
    let access_token = token_access_token(token_data, "Grok")?;
    let expires_in = token_data
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3600);
    auth.access_token = Some(access_token.clone());
    auth.refresh_token = token_data
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| auth.refresh_token.clone());
    auth.expiry_date = Some(current_millis() + expires_in * 1000);
    auth.id_token = token_data
        .get("id_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| auth.id_token.clone());
    auth.token_type = token_data
        .get("token_type")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| auth.token_type.clone());
    auth.expires_in = Some(expires_in);
    Ok(access_token)
}

fn update_auth_value_from_token_response(
    auth: &mut Value,
    token_data: &Value,
) -> Result<(), ProxyError> {
    let access_token = token_access_token(token_data, "OAuth")?;
    set_auth_string(auth, "access_token", access_token);
    if let Some(refresh_token) = token_data.get("refresh_token").and_then(Value::as_str) {
        set_auth_string(auth, "refresh_token", refresh_token.to_string());
    }
    if let Some(id_token) = token_data.get("id_token").and_then(Value::as_str) {
        set_auth_string(auth, "id_token", id_token.to_string());
    }
    if let Some(token_type) = token_data.get("token_type").and_then(Value::as_str) {
        set_auth_string(auth, "token_type", token_type.to_string());
    }
    let expires_in = token_data
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3600);
    set_auth_i64(auth, "expires_in", expires_in);
    set_auth_i64(auth, "expiry_date", current_millis() + expires_in * 1000);
    Ok(())
}

fn token_access_token(token_data: &Value, provider_name: &str) -> Result<String, ProxyError> {
    token_data
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ProxyError::Upstream(format!(
                "{provider_name} token refresh missing access_token"
            ))
        })
}

fn copy_codex_claims_to_auth(auth: &mut CodexAuth) {
    let Some(claims) = auth.id_token.as_deref().and_then(decode_jwt_payload) else {
        return;
    };
    if let Some(email) = claim_string(&claims, "email") {
        auth.email = Some(email);
    }
    if let Some(account_id) =
        claim_string(&claims, "https://api.openai.com/auth.chatgpt_account_id")
    {
        auth.account_id = Some(account_id);
    }
    if let Some(plan_type) = claim_string(&claims, "https://api.openai.com/auth.chatgpt_plan_type")
    {
        auth.plan_type = Some(plan_type);
    }
}

fn copy_codex_claims_to_value(auth: &mut Value) {
    let Some(claims) = auth
        .get("id_token")
        .and_then(Value::as_str)
        .and_then(decode_jwt_payload)
    else {
        return;
    };
    copy_claim_to_value(&claims, auth, "email", "email");
    copy_claim_to_value(
        &claims,
        auth,
        "https://api.openai.com/auth.chatgpt_account_id",
        "account_id",
    );
    copy_claim_to_value(
        &claims,
        auth,
        "https://api.openai.com/auth.chatgpt_plan_type",
        "plan_type",
    );
}

fn decode_jwt_payload(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()
        .or_else(|| {
            base64::engine::general_purpose::URL_SAFE
                .decode(payload)
                .ok()
        })?;
    serde_json::from_slice(&decoded).ok()
}

fn claim_string(claims: &Value, name: &str) -> Option<String> {
    claims
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
}

fn copy_claim_to_value(claims: &Value, auth: &mut Value, claim_name: &str, auth_name: &str) {
    if let Some(value) = claim_string(claims, claim_name) {
        set_auth_string(auth, auth_name, value);
    }
}
