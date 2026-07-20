use base64::Engine;
use serde_json::Value;

use crate::{
    config::{AuthEnabled, CodexAuth, GrokAuth, OneOrMany},
    error::ProxyError,
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
pub(crate) struct AccessToken {
    pub(crate) token: String,
    pub(crate) refreshed: bool,
}

pub(crate) fn responses_endpoint(base_url: &str) -> Result<String, ProxyError> {
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

pub(crate) fn codex_oauth_base_url(provider_base_url: Option<&str>, auth: &CodexAuth) -> String {
    auth.base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| provider_base_url.filter(|value| !value.trim().is_empty()))
        .unwrap_or(DEFAULT_CODEX_BASE_URL)
        .trim_end_matches('/')
        .to_string()
}

pub(crate) fn grok_oauth_base_url(provider_base_url: Option<&str>, auth: &GrokAuth) -> String {
    auth.base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| provider_base_url.filter(|value| !value.trim().is_empty()))
        .unwrap_or(DEFAULT_GROK_BASE_URL)
        .trim_end_matches('/')
        .to_string()
}

pub(crate) fn select_codex_auth(
    auth: &OneOrMany<CodexAuth>,
    start_index: Option<usize>,
    target_attempt: usize,
) -> Result<(usize, CodexAuth), ProxyError> {
    select_auth(auth, start_index, target_attempt, "Codex")
}

pub(crate) fn select_grok_auth(
    auth: &OneOrMany<GrokAuth>,
    start_index: Option<usize>,
    target_attempt: usize,
) -> Result<(usize, GrokAuth), ProxyError> {
    select_auth(auth, start_index, target_attempt, "Grok")
}

pub(crate) async fn codex_access_token(
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

pub(crate) async fn grok_access_token(
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

pub(crate) fn current_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn append_url_path(base_path: &str, path: &str) -> String {
    let base = base_path.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    if base.is_empty() {
        format!("/{path}")
    } else {
        format!("{base}/{path}")
    }
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

fn auth_string(auth: &Value, key: &str) -> Option<String> {
    auth.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn set_auth_string(auth: &mut Value, key: &str, value: String) {
    if let Some(object) = auth.as_object_mut() {
        object.insert(key.to_string(), Value::String(value));
    }
}

fn set_auth_i64(auth: &mut Value, key: &str, value: i64) {
    if let Some(object) = auth.as_object_mut() {
        object.insert(key.to_string(), Value::Number(value.into()));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn responses_endpoint_appends_responses_to_versioned_base_path() {
        let endpoint = responses_endpoint("https://api.example.com/codex/v1").expect("endpoint");
        assert_eq!(endpoint, "https://api.example.com/codex/v1/responses");
    }

    #[test]
    fn responses_endpoint_keeps_explicit_responses_path() {
        let endpoint =
            responses_endpoint("https://api.example.com/v1/responses").expect("endpoint");
        assert_eq!(endpoint, "https://api.example.com/v1/responses");
    }

    #[test]
    fn codex_oauth_base_url_prefers_auth_base_url() {
        let auth: CodexAuth = serde_json::from_value(json!({
            "base_url": "https://chatgpt.example/backend-api/codex/"
        }))
        .unwrap();

        let base_url = codex_oauth_base_url(Some("https://provider.example/codex"), &auth);

        assert_eq!(base_url, "https://chatgpt.example/backend-api/codex");
    }

    #[test]
    fn grok_oauth_base_url_prefers_auth_base_url() {
        let auth: GrokAuth = serde_json::from_value(json!({
            "base_url": "https://cli-chat-proxy.grok.com/v1/"
        }))
        .unwrap();

        let base_url = grok_oauth_base_url(Some("https://api.x.ai/v1"), &auth);

        assert_eq!(base_url, "https://cli-chat-proxy.grok.com/v1");
    }

    #[test]
    fn codex_auth_selection_starts_from_success_cursor() {
        let auth: OneOrMany<CodexAuth> = serde_json::from_value(json!([
            { "access_token": "first" },
            { "access_token": "disabled", "disabled": true },
            { "access_token": "second" }
        ]))
        .unwrap();

        let (idx, selected) = select_codex_auth(&auth, None, 1).unwrap();
        assert_eq!(idx, 0);
        assert_eq!(selected.access_token.as_deref(), Some("first"));

        let (idx, selected) = select_codex_auth(&auth, None, 2).unwrap();
        assert_eq!(idx, 2);
        assert_eq!(selected.access_token.as_deref(), Some("second"));
    }

    #[tokio::test]
    async fn codex_access_token_uses_unexpired_access_token_without_refresh_token() {
        let client = reqwest::Client::new();
        let mut auth: CodexAuth = serde_json::from_value(json!({
            "access_token": "codex-access-token"
        }))
        .unwrap();

        let token = codex_access_token(&client, &mut auth).await.unwrap();

        assert_eq!(token.token, "codex-access-token");
        assert!(!token.refreshed);
    }

    #[tokio::test]
    async fn grok_access_token_uses_access_token_without_expiry_check() {
        let client = reqwest::Client::new();
        let mut auth: GrokAuth = serde_json::from_value(json!({
            "access_token": "expired-grok-token",
            "expiry_date": current_millis() - 1_000
        }))
        .unwrap();

        let token = grok_access_token(&client, &mut auth).await.unwrap();

        assert_eq!(token.token, "expired-grok-token");
        assert!(!token.refreshed);
    }
}
