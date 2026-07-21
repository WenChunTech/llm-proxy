use std::{collections::HashSet, time::Duration};

use bytes::Bytes;
use futures_util::TryStreamExt;
use salvo::{
    http::{HeaderValue, StatusCode, header},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    app::JSON_MAX_SIZE,
    config::{BaseProviderConfig, Config, OneOrMany, ProviderConfig},
    error::ProxyError,
    provider::{
        UpstreamResponse, oauth,
        types::{HeaderMap, ProviderType},
    },
    state::{AppSnapshot, AppState},
};

use super::{apply_headers, render_error, state_from_depot};

#[handler]
pub(super) async fn api_health(depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let snapshot = state.snapshot().await;
    res.render(Json(json!({
        "status": "ok",
        "port": snapshot.config.port,
        "bind": snapshot.config.bind_addr(),
        "configured_models": snapshot.registry.configured_models().len(),
    })));
}

#[handler]
pub(super) async fn api_config(depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let snapshot = state.snapshot().await;
    res.render(Json(config_payload(&snapshot)));
}

#[handler]
pub(super) async fn api_update_config(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<DashboardConfig>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };

    let current = state.snapshot().await.config.as_ref().clone();
    let next = payload.apply_to(current);
    match state.update_config(next).await {
        Ok(()) => {
            let snapshot = state.snapshot().await;
            res.render(Json(config_payload(&snapshot)));
        }
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub(super) async fn api_models(depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let snapshot = state.snapshot().await;
    res.render(Json(models_payload(&snapshot)));
}

#[handler]
pub(super) async fn api_provider_models(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<ProviderModelsRequest>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };

    match fetch_provider_models(&state.http, &payload).await {
        Ok(result) => res.render(Json(json!({
            "object": "list",
            "endpoint": result.endpoint,
            "data": result.model_ids
        }))),
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub(super) async fn api_provider_test(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<ProviderTestRequest>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };

    if payload.stream {
        match stream_provider_model(&state, payload).await {
            Ok(response) => write_provider_test_stream_response(res, response),
            Err(error) => render_error(res, error),
        }
        return;
    }

    match test_provider_model(&state, payload).await {
        Ok(result) => res.render(Json(result)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub(super) async fn api_validate_codex_auths(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<AuthValidateRequest>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };
    match validate_auths(&state, payload, ProviderType::Codex).await {
        Ok(result) => res.render(Json(result)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub(super) async fn api_validate_grok_auths(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<AuthValidateRequest>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };
    match validate_auths(&state, payload, ProviderType::Grok).await {
        Ok(result) => res.render(Json(result)),
        Err(error) => render_error(res, error),
    }
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DashboardPayload {
    port: u16,
    providers: Vec<DashboardProvider>,
    model_priority: Vec<String>,
    fallback_models: Vec<String>,
    model_aliases: std::collections::HashMap<String, String>,
    retry: DashboardRetry,
    api_key: String,
    api_key_enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct DashboardConfig {
    #[serde(default)]
    port: Option<u16>,
    providers: Vec<DashboardProvider>,
    model_priority: Vec<String>,
    fallback_models: Vec<String>,
    #[serde(default)]
    model_aliases: std::collections::HashMap<String, String>,
    retry: DashboardRetry,
    #[serde(default)]
    api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct DashboardProvider {
    id: String,
    kind: String,
    name: String,
    enabled: bool,
    base_url: String,
    api_key: String,
    models: Vec<String>,
    #[serde(default)]
    headers: std::collections::HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProviderModelsRequest {
    kind: String,
    base_url: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    auth: Option<Value>,
}

#[derive(Debug, Clone)]
struct ProviderModelsResult {
    endpoint: String,
    model_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProviderTestRequest {
    provider: DashboardProvider,
    model: String,
    prompt: Option<String>,
    #[serde(default = "default_provider_test_stream")]
    stream: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct AuthValidateRequest {
    #[serde(default)]
    config: Option<DashboardAuthConfig>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default, alias = "providerIndices")]
    provider_indices: Option<Vec<usize>>,
    #[serde(default)]
    targets: Option<Vec<AuthValidateTarget>>,
}

#[derive(Debug, Clone, Deserialize)]
struct DashboardAuthConfig {
    #[serde(default)]
    codex: Vec<DashboardAuthProvider>,
    #[serde(default)]
    grok: Vec<DashboardAuthProvider>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardAuthProvider {
    #[serde(default = "default_dashboard_provider_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub auth: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthValidateTarget {
    #[serde(alias = "provider_index")]
    provider_index: usize,
    #[serde(alias = "auth_index")]
    auth_index: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthValidateResponse {
    success: bool,
    data: AuthValidatePayload,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthValidatePayload {
    model: String,
    provider_indices: Vec<usize>,
    targets: Vec<String>,
    total: usize,
    checked: usize,
    valid: usize,
    invalid: usize,
    skipped: usize,
    rate_limited: usize,
    refreshed: usize,
    results: Vec<AuthValidateResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthValidateResult {
    provider_index: usize,
    auth_index: usize,
    auth_count: usize,
    is_auth_array: bool,
    label: String,
    disabled: bool,
    skipped: bool,
    valid: bool,
    reason: String,
    status_code: u16,
    error_message: String,
    refreshed: bool,
    auth: Value,
}

#[derive(Debug, Clone, Serialize)]
struct ProviderTestResult {
    ok: bool,
    status: u16,
    provider: String,
    model: String,
    stream: bool,
    raw_body: String,
    body_preview: String,
}

enum ProviderTestStreamResponse {
    Upstream(UpstreamResponse),
    Direct {
        status: u16,
        headers: HeaderMap,
        response: reqwest::Response,
    },
}

fn default_provider_test_stream() -> bool {
    true
}

fn default_dashboard_provider_enabled() -> bool {
    true
}

const CODEX_VALIDATION_MODEL: &str = "gpt-5.4";
const GROK_VALIDATION_MODEL: &str = "grok-4.5";

#[derive(Debug, Clone)]
struct AuthValidationProbe {
    valid: bool,
    reason: String,
    status_code: u16,
    error_message: String,
}

impl AuthValidationProbe {
    fn skipped(reason: &str, message: impl Into<String>) -> Self {
        Self {
            valid: false,
            reason: reason.to_string(),
            status_code: 0,
            error_message: message.into(),
        }
    }
}

async fn validate_auths(
    state: &AppState,
    request: AuthValidateRequest,
    provider_type: ProviderType,
) -> Result<AuthValidateResponse, ProxyError> {
    let model = request
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(match provider_type {
            ProviderType::Codex => CODEX_VALIDATION_MODEL,
            ProviderType::Grok => GROK_VALIDATION_MODEL,
            _ => unreachable!("auth validation is only supported for Codex and Grok"),
        })
        .to_string();
    let providers = auth_validation_providers(state, request.config, provider_type).await;
    let provider_indices = request
        .provider_indices
        .unwrap_or_else(|| (0..providers.len()).collect());
    let target_filter: Option<HashSet<(usize, usize)>> = request.targets.as_ref().map(|targets| {
        targets
            .iter()
            .map(|target| (target.provider_index, target.auth_index))
            .collect()
    });
    let target_labels = target_filter
        .as_ref()
        .map(|targets| {
            let mut labels: Vec<String> = targets
                .iter()
                .map(|(provider_index, auth_index)| format!("{provider_index}:{auth_index}"))
                .collect();
            labels.sort();
            labels
        })
        .unwrap_or_default();

    let mut results = Vec::new();
    for provider_index in &provider_indices {
        let Some(provider) = providers.get(*provider_index) else {
            continue;
        };
        let auth_items = auth_validation_items(provider.auth.as_ref());
        let auth_count = auth_items.len();
        if auth_items.is_empty() {
            results.push(AuthValidateResult {
                provider_index: *provider_index,
                auth_index: 0,
                auth_count: 0,
                is_auth_array: provider.auth.as_ref().is_some_and(Value::is_array),
                label: auth_validation_label(provider_type, *provider_index, 0, 0, None),
                disabled: !provider.enabled,
                skipped: true,
                valid: false,
                reason: "no_auth".to_string(),
                status_code: 0,
                error_message: "auth JSON is empty".to_string(),
                refreshed: false,
                auth: Value::Null,
            });
            continue;
        }

        for (auth_index, auth) in auth_items.into_iter().enumerate() {
            if target_filter
                .as_ref()
                .is_some_and(|targets| !targets.contains(&(*provider_index, auth_index)))
            {
                continue;
            }
            let mut auth = auth.clone();
            let disabled = auth_disabled(&auth) || !provider.enabled;
            let is_auth_array = provider.auth.as_ref().is_some_and(Value::is_array);
            let label = auth_validation_label(
                provider_type,
                *provider_index,
                auth_index,
                auth_count,
                Some(&auth),
            );
            let (probe, refreshed) =
                validate_single_auth(&state.http, provider_type, provider, &mut auth, &model).await;
            if probe.reason == "rate_limited" {
                set_auth_bool(&mut auth, "disabled", true);
            }
            results.push(AuthValidateResult {
                provider_index: *provider_index,
                auth_index,
                auth_count,
                is_auth_array,
                label,
                disabled: auth_disabled(&auth) || disabled,
                skipped: false,
                valid: probe.valid,
                reason: probe.reason,
                status_code: probe.status_code,
                error_message: probe.error_message,
                refreshed,
                auth,
            });
        }
    }

    let checked = results.iter().filter(|result| !result.skipped).count();
    let valid = results.iter().filter(|result| result.valid).count();
    let rate_limited = results
        .iter()
        .filter(|result| result.reason == "rate_limited")
        .count();
    let skipped = results.iter().filter(|result| result.skipped).count();
    let refreshed = results.iter().filter(|result| result.refreshed).count();
    let invalid = results
        .iter()
        .filter(|result| !result.valid && !result.skipped)
        .count();

    Ok(AuthValidateResponse {
        success: true,
        data: AuthValidatePayload {
            model,
            provider_indices,
            targets: target_labels,
            total: results.len(),
            checked,
            valid,
            invalid,
            skipped,
            rate_limited,
            refreshed,
            results,
        },
    })
}

async fn auth_validation_providers(
    state: &AppState,
    request_config: Option<DashboardAuthConfig>,
    provider_type: ProviderType,
) -> Vec<DashboardAuthProvider> {
    if let Some(config) = request_config {
        return match provider_type {
            ProviderType::Codex => config.codex,
            ProviderType::Grok => config.grok,
            _ => Vec::new(),
        };
    }

    let snapshot = state.snapshot().await;
    match provider_type {
        ProviderType::Codex => snapshot
            .config
            .providers
            .codex
            .iter()
            .map(|provider| DashboardAuthProvider {
                enabled: provider.base.enabled,
                base_url: provider.base.base_url.clone(),
                auth: serde_json::to_value(&provider.auth).ok(),
            })
            .collect(),
        ProviderType::Grok => snapshot
            .config
            .providers
            .grok
            .iter()
            .map(|provider| DashboardAuthProvider {
                enabled: provider.base.enabled,
                base_url: provider.base.base_url.clone(),
                auth: serde_json::to_value(&provider.auth).ok(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn auth_validation_items(auth: Option<&Value>) -> Vec<&Value> {
    match auth {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Object(_)) => auth.into_iter().collect(),
        _ => Vec::new(),
    }
}

fn auth_validation_label(
    provider_type: ProviderType,
    provider_index: usize,
    auth_index: usize,
    auth_count: usize,
    auth: Option<&Value>,
) -> String {
    let provider = match provider_type {
        ProviderType::Codex => "Codex",
        ProviderType::Grok => "Grok",
        _ => "Provider",
    };
    let email = auth
        .and_then(|auth| auth.get("email").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty());
    if let Some(email) = email {
        return email.to_string();
    }
    if auth_count > 1 {
        format!(
            "{provider} #{} auth #{}",
            provider_index + 1,
            auth_index + 1
        )
    } else {
        format!("{provider} #{}", provider_index + 1)
    }
}

async fn validate_single_auth(
    client: &reqwest::Client,
    provider_type: ProviderType,
    provider: &DashboardAuthProvider,
    auth: &mut Value,
    model: &str,
) -> (AuthValidationProbe, bool) {
    if !auth.is_object() {
        return (
            AuthValidationProbe::skipped("invalid_auth_json", "auth item must be a JSON object"),
            false,
        );
    }

    let mut refreshed = false;
    if oauth::should_refresh_value(provider_type, auth) {
        match refresh_validation_auth(client, provider_type, auth).await {
            Ok(()) => refreshed = true,
            Err(message) => {
                return (
                    AuthValidationProbe::skipped("refresh_failed", message),
                    refreshed,
                );
            }
        }
    }

    let Some(token) = auth_string(auth, "access_token") else {
        return (
            AuthValidationProbe::skipped(
                "missing_access_token",
                "access_token is missing and refresh_token is not configured",
            ),
            refreshed,
        );
    };

    let probe = probe_validation_auth(client, provider_type, provider, auth, &token, model).await;
    if probe.reason == "invalid_auth" && auth_string(auth, "refresh_token").is_some() && !refreshed
    {
        match refresh_validation_auth(client, provider_type, auth).await {
            Ok(()) => {
                refreshed = true;
                if let Some(token) = auth_string(auth, "access_token") {
                    return (
                        probe_validation_auth(client, provider_type, provider, auth, &token, model)
                            .await,
                        refreshed,
                    );
                }
            }
            Err(message) => {
                return (
                    AuthValidationProbe::skipped("refresh_failed", message),
                    refreshed,
                );
            }
        }
    }
    (probe, refreshed)
}

async fn probe_validation_auth(
    client: &reqwest::Client,
    provider_type: ProviderType,
    provider: &DashboardAuthProvider,
    auth: &Value,
    token: &str,
    model: &str,
) -> AuthValidationProbe {
    let default_base_url = match provider_type {
        ProviderType::Codex => oauth::DEFAULT_CODEX_BASE_URL,
        ProviderType::Grok => oauth::DEFAULT_GROK_BASE_URL,
        _ => "",
    };
    let base_url = validation_auth_base_url(provider_type, provider, auth, default_base_url);
    let endpoint = match build_provider_responses_endpoint(base_url) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            return AuthValidationProbe::skipped("invalid_base_url", error.to_string());
        }
    };
    let body = json!({
        "model": model,
        "input": [{"content": "hello", "role": "user"}],
        "instructions": "",
        "store": false,
        "stream": true,
    });
    let mut builder = client
        .post(&endpoint)
        .bearer_auth(token)
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .header("connection", "Keep-Alive")
        .timeout(Duration::from_secs(30))
        .json(&body);
    if provider_type == ProviderType::Codex {
        builder = builder
            .header("user-agent", oauth::CODEX_USER_AGENT)
            .header("originator", "codex-tui");
        if let Some(account_id) = auth
            .get("account_id")
            .or_else(|| auth.get("chatgpt_account_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            builder = builder.header("chatgpt-account-id", account_id);
        }
    }
    if matches!(provider_type, ProviderType::Codex | ProviderType::Grok)
        && let Some(headers) = auth.get("headers").and_then(Value::as_object)
    {
        for (name, value) in headers {
            if let Some(value) = value.as_str().filter(|value| !value.trim().is_empty()) {
                builder = builder.header(name, value);
            }
        }
    }

    match builder.send().await {
        Ok(response) => {
            let status_code = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            classify_validation_response(provider_type, status_code, &body)
        }
        Err(error) => {
            let status_code = error.status().map(|status| status.as_u16()).unwrap_or(0);
            let upstream_url = error
                .url()
                .map(|url| url.as_str().to_string())
                .unwrap_or(endpoint);
            tracing::warn!(
                provider = ?provider_type,
                status_code,
                upstream_url = %upstream_url,
                error = %error,
                "auth validation request failed"
            );
            AuthValidationProbe {
                valid: false,
                reason: "network_error".to_string(),
                status_code,
                error_message: error.to_string(),
            }
        }
    }
}

fn classify_validation_response(
    provider_type: ProviderType,
    status_code: u16,
    body: &str,
) -> AuthValidationProbe {
    let error_type = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value.get("error").cloned())
        .map(|error| {
            let error_type = error
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let error_code = error
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            (error_type, error_code, message)
        });
    let lower_body = body.to_lowercase();
    let invalid_auth = status_code == 401
        || error_type
            .as_ref()
            .is_some_and(|(error_type, error_code, _)| {
                error_type == "authentication_error" || error_code == "invalid_api_key"
            })
        || lower_body.contains("invalid or expired token")
        || (provider_type == ProviderType::Grok
            && (lower_body.contains("invalid_api_key") || lower_body.contains("unauthorized")));
    if invalid_auth {
        return AuthValidationProbe {
            valid: false,
            reason: "invalid_auth".to_string(),
            status_code,
            error_message: validation_error_message(error_type.as_ref(), body),
        };
    }

    let (valid, reason) = match status_code {
        200 | 201 => (true, "ok"),
        402 => (false, "payment_required"),
        403 => (false, "forbidden"),
        429 => (true, "rate_limited"),
        400..=499 => (true, "request_error"),
        500..=599 => (true, "server_error"),
        _ => (false, "unexpected"),
    };
    AuthValidationProbe {
        valid,
        reason: reason.to_string(),
        status_code,
        error_message: validation_error_message(error_type.as_ref(), body),
    }
}

fn validation_error_message(error_type: Option<&(String, String, String)>, body: &str) -> String {
    if let Some((_, _, message)) = error_type
        && !message.trim().is_empty()
    {
        return message.clone();
    }
    body.chars().take(500).collect()
}

pub fn validation_auth_base_url<'a>(
    provider_type: ProviderType,
    provider: &'a DashboardAuthProvider,
    auth: &'a Value,
    default_base_url: &'a str,
) -> &'a str {
    let provider_base_url = provider.base_url.trim();
    let auth_base_url = auth
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if provider_type == ProviderType::Grok {
        return auth_base_url
            .or_else(|| (!provider_base_url.is_empty()).then_some(provider_base_url))
            .unwrap_or(default_base_url);
    }

    (!provider_base_url.is_empty())
        .then_some(provider_base_url)
        .or(auth_base_url)
        .unwrap_or(default_base_url)
}

async fn refresh_validation_auth(
    client: &reqwest::Client,
    provider_type: ProviderType,
    auth: &mut Value,
) -> Result<(), String> {
    match provider_type {
        ProviderType::Codex => oauth::refresh_codex_auth_value(client, auth)
            .await
            .map_err(|error| error.to_string()),
        ProviderType::Grok => oauth::refresh_grok_auth_value(client, auth)
            .await
            .map_err(|error| error.to_string()),
        _ => Ok(()),
    }
}

fn auth_string(auth: &Value, key: &str) -> Option<String> {
    auth.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn auth_disabled(auth: &Value) -> bool {
    auth.get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn auth_object_mut(auth: &mut Value) -> Option<&mut Map<String, Value>> {
    auth.as_object_mut()
}

fn set_auth_bool(auth: &mut Value, key: &str, value: bool) {
    if let Some(object) = auth_object_mut(auth) {
        object.insert(key.to_string(), Value::Bool(value));
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct DashboardRetry {
    max_retries: usize,
    backoff_step_ms: u64,
}

fn config_payload(snapshot: &AppSnapshot) -> DashboardPayload {
    DashboardPayload {
        port: snapshot.config.port,
        providers: dashboard_providers(&snapshot.config),
        model_priority: snapshot.config.model_priority.clone(),
        fallback_models: snapshot.config.fallback_models.clone(),
        model_aliases: snapshot.config.model_aliases.clone(),
        retry: DashboardRetry {
            max_retries: snapshot.config.retry.max_retries,
            backoff_step_ms: snapshot.config.retry.backoff_step_ms,
        },
        api_key: snapshot.config.api_key.clone().unwrap_or_default(),
        api_key_enabled: snapshot
            .config
            .api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty()),
    }
}

pub(super) fn models_payload(snapshot: &AppSnapshot) -> Value {
    let mut data: Vec<Value> = snapshot
        .registry
        .configured_models()
        .into_iter()
        .map(|(id, provider)| {
            json!({
                "id": id,
                "object": "model",
                "owned_by": provider.as_str()
            })
        })
        .collect();
    let mut aliases: Vec<_> = snapshot.config.model_aliases.iter().collect();
    aliases.sort_by(|a, b| a.0.cmp(b.0));
    for (alias, target) in aliases {
        let alias = alias.trim();
        let target = target.trim();
        if alias.is_empty() || target.is_empty() {
            continue;
        }
        data.push(json!({
            "id": alias,
            "object": "model",
            "owned_by": "alias",
            "root": target
        }));
    }
    json!({ "object": "list", "data": data })
}

async fn fetch_provider_models(
    client: &reqwest::Client,
    request: &ProviderModelsRequest,
) -> Result<ProviderModelsResult, ProxyError> {
    let base_url = request.base_url.trim();
    let api_key = request.api_key.trim();
    if base_url.is_empty() {
        return Err(ProxyError::InvalidRequest(
            "base_url is required".to_string(),
        ));
    }

    let endpoint = build_provider_models_endpoint(base_url, &request.kind)?;
    let mut builder = client
        .get(&endpoint)
        .header("accept", "application/json")
        .timeout(Duration::from_secs(8));
    if !api_key.is_empty() {
        builder = builder.bearer_auth(api_key).header("x-api-key", api_key);
    } else if let Some(token) = auth_access_token(request.auth.as_ref()) {
        builder = builder.bearer_auth(token);
    }
    if request.kind == "claude" {
        builder = builder.header("anthropic-version", "2023-06-01");
    }
    if request.kind == "gemini" {
        builder = builder.header("x-goog-api-key", api_key);
    }
    if request.kind == "codex" {
        builder = builder
            .header("user-agent", oauth::CODEX_USER_AGENT)
            .header("originator", "codex-tui");
    }
    if matches!(request.kind.as_str(), "codex" | "grok")
        && let Some(headers) = auth_headers(request.auth.as_ref())
    {
        for (name, value) in headers {
            builder = builder.header(name, value);
        }
    }
    for (name, value) in &request.headers {
        let value = value.trim();
        if !value.is_empty() {
            builder = builder.header(name, value);
        }
    }

    let response = builder.send().await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(ProxyError::Upstream(format!(
            "model list request failed: {status} {body}"
        )));
    }

    let value: Value = serde_json::from_str(&body)?;
    let mut model_ids = parse_provider_model_list(&value);
    model_ids.sort();
    model_ids.dedup();
    Ok(ProviderModelsResult {
        endpoint,
        model_ids,
    })
}

fn auth_access_token(auth_json: Option<&Value>) -> Option<&str> {
    auth_values(auth_json)
        .into_iter()
        .filter(|item| {
            !item
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .find_map(|item| item.get("access_token").and_then(Value::as_str))
        .filter(|token| !token.trim().is_empty())
}

fn auth_headers(auth_json: Option<&Value>) -> Option<Vec<(&str, &str)>> {
    let selected_auth = auth_values(auth_json).into_iter().find(|item| {
        !item
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    })?;
    let headers = selected_auth.get("headers")?.as_object()?;
    let mut out = Vec::new();
    for (name, value) in headers {
        let Some(value) = value.as_str().filter(|value| !value.trim().is_empty()) else {
            continue;
        };
        out.push((name.as_str(), value));
    }
    Some(out)
}

fn auth_values(auth_json: Option<&Value>) -> Vec<&Value> {
    match auth_json {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Object(_)) => auth_json.into_iter().collect(),
        _ => Vec::new(),
    }
}

pub fn build_provider_models_endpoint(
    base_url: &str,
    provider_kind: &str,
) -> Result<String, ProxyError> {
    let mut url = reqwest::Url::parse(base_url)
        .map_err(|_| ProxyError::InvalidRequest("invalid base_url".to_string()))?;
    let pathname = url.path().trim_end_matches('/').to_string();

    if pathname.ends_with("/models") {
        url.set_path(if pathname.is_empty() {
            "/models"
        } else {
            &pathname
        });
    } else {
        let is_openai_style = ProviderType::from_config_id(provider_kind)
            .is_some_and(ProviderType::uses_openai_models_endpoint);
        let has_version_path = has_version_path(&pathname);
        let suffix = if !is_openai_style && !has_version_path {
            "v1/models"
        } else {
            "models"
        };
        url.set_path(&append_url_path(&pathname, suffix));
    }

    Ok(url.to_string())
}

pub fn build_provider_responses_endpoint(base_url: &str) -> Result<String, ProxyError> {
    oauth::responses_endpoint(base_url)
}

fn has_version_path(pathname: &str) -> bool {
    let Some(segment) = pathname.rsplit('/').find(|segment| !segment.is_empty()) else {
        return false;
    };
    let Some(version) = segment.strip_prefix('v') else {
        return false;
    };
    !version.is_empty() && version.chars().all(|item| item.is_ascii_digit())
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

fn parse_provider_model_list(value: &Value) -> Vec<String> {
    let mut model_ids = parse_model_data_list(value);
    model_ids.extend(parse_gemini_model_list(value));
    model_ids
}

fn parse_model_data_list(value: &Value) -> Vec<String> {
    value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .filter(|id| !id.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_gemini_model_list(value: &Value) -> Vec<String> {
    value
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("name").and_then(Value::as_str))
        .filter_map(|name| name.strip_prefix("models/").or(Some(name)))
        .filter(|id| !id.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

async fn test_provider_model(
    state: &AppState,
    request: ProviderTestRequest,
) -> Result<ProviderTestResult, ProxyError> {
    let model = request.model.trim();
    if model.is_empty() {
        return Err(ProxyError::InvalidRequest("model is required".to_string()));
    }

    let provider_type = request.provider.provider_type()?;
    let source_type = provider_type.response_protocol();
    let prompt = request
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("hello");
    let stream = request.stream;
    let body = hello_body(source_type, model, prompt, stream);
    let provider_request =
        state
            .providers
            .prepare_request(provider_type, body, source_type, stream)?;
    if matches!(provider_type, ProviderType::Codex | ProviderType::Grok)
        && !request.provider.api_key.trim().is_empty()
    {
        return test_api_key_responses_provider(
            &state.http,
            &request.provider,
            provider_request,
            model,
            stream,
        )
        .await;
    }

    let config = request.provider.provider_config()?;
    let headers = HeaderMap::new();
    let upstream = tokio::time::timeout(
        Duration::from_secs(30),
        state.providers.send_request(crate::provider::SendRequest {
            state: None,
            client: &state.http,
            is_streaming: stream,
            provider_type,
            request: provider_request,
            config: &config,
            config_index: 0,
            forwarded_headers: &headers,
            model,
            auth_start_index: None,
            target_attempt: 1,
        }),
    )
    .await
    .map_err(|_| ProxyError::Upstream("provider test timed out".to_string()))??;

    let status = upstream.status();
    let body = collect_test_response_body(upstream).await?;
    let raw_body = response_text(&body);
    let body_preview = response_preview(&body);

    Ok(ProviderTestResult {
        ok: (200..300).contains(&status),
        status,
        provider: request.provider.kind,
        model: model.to_string(),
        stream,
        raw_body,
        body_preview,
    })
}

async fn stream_provider_model(
    state: &AppState,
    request: ProviderTestRequest,
) -> Result<ProviderTestStreamResponse, ProxyError> {
    let model = request.model.trim();
    if model.is_empty() {
        return Err(ProxyError::InvalidRequest("model is required".to_string()));
    }

    let provider_type = request.provider.provider_type()?;
    let source_type = provider_type.response_protocol();
    let prompt = request
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("hello");
    let stream = request.stream;
    let body = hello_body(source_type, model, prompt, stream);
    let provider_request =
        state
            .providers
            .prepare_request(provider_type, body, source_type, stream)?;

    if matches!(provider_type, ProviderType::Codex | ProviderType::Grok)
        && !request.provider.api_key.trim().is_empty()
    {
        return stream_api_key_responses_provider(
            &state.http,
            &request.provider,
            provider_request,
            stream,
        )
        .await;
    }

    let config = request.provider.provider_config()?;
    let headers = HeaderMap::new();
    let upstream = tokio::time::timeout(
        Duration::from_secs(30),
        state.providers.send_request(crate::provider::SendRequest {
            state: None,
            client: &state.http,
            is_streaming: stream,
            provider_type,
            request: provider_request,
            config: &config,
            config_index: 0,
            forwarded_headers: &headers,
            model,
            auth_start_index: None,
            target_attempt: 1,
        }),
    )
    .await
    .map_err(|_| ProxyError::Upstream("provider test timed out".to_string()))??;

    Ok(ProviderTestStreamResponse::Upstream(upstream))
}

async fn stream_api_key_responses_provider(
    client: &reqwest::Client,
    provider: &DashboardProvider,
    body: Value,
    stream: bool,
) -> Result<ProviderTestStreamResponse, ProxyError> {
    let endpoint = build_provider_responses_endpoint(&provider.base_url)?;
    let api_key = provider.api_key.trim();
    if api_key.is_empty() {
        return Err(ProxyError::InvalidRequest(
            "api_key is required".to_string(),
        ));
    }

    let mut builder = client
        .post(&endpoint)
        .bearer_auth(api_key)
        .header("content-type", "application/json")
        .header(
            "accept",
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        )
        .timeout(Duration::from_secs(30))
        .json(&body);
    if provider.kind == "codex" {
        builder = builder
            .header("user-agent", oauth::CODEX_USER_AGENT)
            .header("originator", "codex-tui");
    }
    builder = apply_provider_extra_headers(builder, provider);

    let response = builder.send().await?;
    let status = response.status().as_u16();
    let headers = response_headers(response.headers());
    Ok(ProviderTestStreamResponse::Direct {
        status,
        headers,
        response,
    })
}

fn write_provider_test_stream_response(res: &mut Response, response: ProviderTestStreamResponse) {
    match response {
        ProviderTestStreamResponse::Upstream(upstream) => {
            apply_headers(res, upstream.headers());
            res.status_code(StatusCode::from_u16(upstream.status()).unwrap_or(StatusCode::OK));
            match upstream {
                UpstreamResponse::NonStream { body, .. } => {
                    res.body(body);
                }
                UpstreamResponse::Stream { response, .. } => {
                    res.headers_mut().insert(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("text/event-stream"),
                    );
                    let stream = response.bytes_stream().map_err(std::io::Error::other);
                    res.stream(stream);
                }
            }
        }
        ProviderTestStreamResponse::Direct {
            status,
            headers,
            response,
        } => {
            apply_headers(res, &headers);
            res.status_code(StatusCode::from_u16(status).unwrap_or(StatusCode::OK));
            if !headers.contains_key("content-type") {
                res.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/event-stream"),
                );
            }
            let stream = response.bytes_stream().map_err(std::io::Error::other);
            res.stream(stream);
        }
    }
}


fn apply_provider_extra_headers(
    mut builder: reqwest::RequestBuilder,
    provider: &DashboardProvider,
) -> reqwest::RequestBuilder {
    for (name, value) in &provider.headers {
        let value = value.trim();
        if !value.is_empty() {
            builder = builder.header(name, value);
        }
    }
    if matches!(provider.kind.as_str(), "codex" | "grok")
        && let Some(headers) = auth_headers(provider.auth.as_ref())
    {
        for (name, value) in headers {
            builder = builder.header(name, value);
        }
    }
    builder
}

async fn test_api_key_responses_provider(
    client: &reqwest::Client,
    provider: &DashboardProvider,
    body: Value,
    model: &str,
    stream: bool,
) -> Result<ProviderTestResult, ProxyError> {
    let endpoint = build_provider_responses_endpoint(&provider.base_url)?;
    let api_key = provider.api_key.trim();
    if api_key.is_empty() {
        return Err(ProxyError::InvalidRequest(
            "api_key is required".to_string(),
        ));
    }

    let mut builder = client
        .post(&endpoint)
        .bearer_auth(api_key)
        .header("content-type", "application/json")
        .header(
            "accept",
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        )
        .timeout(Duration::from_secs(30))
        .json(&body);
    if provider.kind == "codex" {
        builder = builder
            .header("user-agent", oauth::CODEX_USER_AGENT)
            .header("originator", "codex-tui");
    }
    builder = apply_provider_extra_headers(builder, provider);

    let response = builder.send().await?;
    let status = response.status().as_u16();
    let body = response.bytes().await?;
    let raw_body = response_text(&body);

    Ok(ProviderTestResult {
        ok: (200..300).contains(&status),
        status,
        provider: provider.kind.clone(),
        model: model.to_string(),
        stream,
        raw_body,
        body_preview: response_preview(&body),
    })
}

async fn collect_test_response_body(response: UpstreamResponse) -> Result<Bytes, ProxyError> {
    match response {
        UpstreamResponse::NonStream { body, .. } => Ok(body),
        UpstreamResponse::Stream { response, .. } => Ok(response.bytes().await?),
    }
}

fn hello_body(target: ProviderType, model: &str, prompt: &str, stream: bool) -> Value {
    match target {
        ProviderType::Chat => json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": stream,
            "max_tokens": 16
        }),
        ProviderType::Responses => json!({
            "model": model,
            "input": prompt,
            "stream": stream,
            "max_output_tokens": 16
        }),
        ProviderType::Claude => json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": stream,
            "max_tokens": 16
        }),
        ProviderType::Gemini => json!({
            "contents": [{
                "role": "user",
                "parts": [{"text": prompt}]
            }]
        }),
        ProviderType::Codex | ProviderType::Grok => json!({
            "model": model,
            "input": prompt,
            "stream": stream,
            "max_output_tokens": 16
        }),
    }
}

fn response_text(body: &[u8]) -> String {
    String::from_utf8_lossy(body).into_owned()
}

fn response_headers(headers: &reqwest::header::HeaderMap) -> HeaderMap {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect()
}

fn response_preview(body: &[u8]) -> String {
    let text = response_text(body);
    text.chars().take(500).collect()
}

fn dashboard_providers(config: &Config) -> Vec<DashboardProvider> {
    let mut providers = Vec::new();
    for (provider_type, index, provider_config) in config.providers.iter_configs() {
        providers.push(dashboard_provider_from_config(
            provider_type,
            index,
            &provider_config,
        ));
    }
    providers
}

fn dashboard_provider_from_config(
    kind: ProviderType,
    index: usize,
    config: &ProviderConfig,
) -> DashboardProvider {
    match config {
        ProviderConfig::OpenAiChat(config) => provider_payload(kind, index, &config.base, None),
        ProviderConfig::OpenAiResponses(config) => {
            provider_payload(kind, index, &config.base, None)
        }
        ProviderConfig::Claude(config) => provider_payload(kind, index, &config.base, None),
        ProviderConfig::Gemini(config) => provider_payload(kind, index, &config.base, None),
        ProviderConfig::Codex(config) => provider_payload(
            kind,
            index,
            &config.base,
            serde_json::to_value(&config.auth).ok(),
        ),
        ProviderConfig::Grok(config) => provider_payload(
            kind,
            index,
            &config.base,
            serde_json::to_value(&config.auth).ok(),
        ),
    }
}

fn provider_payload(
    kind: ProviderType,
    index: usize,
    base: &BaseProviderConfig,
    auth_json: Option<Value>,
) -> DashboardProvider {
    DashboardProvider {
        id: format!("{}:{index}", kind.as_str()),
        kind: kind.as_str().to_string(),
        name: provider_name(kind, &base.base_url, index),
        enabled: base.enabled,
        base_url: base.base_url.clone(),
        api_key: base.api_key.clone(),
        models: base.models.clone(),
        headers: base.headers.clone(),
        auth: auth_json,
    }
}

fn provider_name(kind: ProviderType, base_url: &str, index: usize) -> String {
    let label = kind.display_name();
    if base_url.trim().is_empty() {
        format!("{label} #{index}", index = index + 1)
    } else {
        format!("{label} - {base_url}")
    }
}

impl DashboardConfig {
    fn apply_to(self, mut config: Config) -> Config {
        if let Some(port) = self.port {
            config.port = port;
        }
        config.model_priority = self.model_priority;
        config.fallback_models = self.fallback_models;
        config.model_aliases = self.model_aliases;
        config.api_key = self
            .api_key
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        config.retry.max_retries = self.retry.max_retries;
        config.retry.backoff_step_ms = self.retry.backoff_step_ms;

        let providers = self.providers.into_iter().filter_map(|provider| {
            match provider.persisted_provider_config() {
                Ok(Some(config)) => Some(config),
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!(error = %error, "invalid dashboard provider config skipped");
                    None
                }
            }
        });
        config.providers = crate::config::ProviderGroups::from_configs(providers);
        config
    }
}

impl DashboardProvider {
    fn base(&self) -> BaseProviderConfig {
        BaseProviderConfig {
            enabled: self.enabled,
            models: self.models.clone(),
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            headers: self.headers.clone(),
        }
    }

    fn provider_type(&self) -> Result<ProviderType, ProxyError> {
        ProviderType::from_config_id(&self.kind).ok_or_else(|| {
            ProxyError::InvalidRequest(format!("unsupported provider kind: {}", self.kind))
        })
    }

    fn provider_config(&self) -> Result<ProviderConfig, ProxyError> {
        self.provider_type().and_then(|kind| match kind {
            ProviderType::Chat => Ok(ProviderConfig::OpenAiChat(
                crate::config::OpenAiChatConfig { base: self.base() },
            )),
            ProviderType::Responses => Ok(ProviderConfig::OpenAiResponses(
                crate::config::OpenAiResponsesConfig { base: self.base() },
            )),
            ProviderType::Claude => Ok(ProviderConfig::Claude(crate::config::ClaudeConfig {
                base: self.base(),
            })),
            ProviderType::Gemini => Ok(ProviderConfig::Gemini(crate::config::GeminiConfig {
                base: self.base(),
            })),
            ProviderType::Codex => {
                self.codex_config()?
                    .map(ProviderConfig::Codex)
                    .ok_or_else(|| {
                        ProxyError::InvalidRequest("Codex auth JSON is required".to_string())
                    })
            }
            ProviderType::Grok => self
                .grok_config()?
                .map(ProviderConfig::Grok)
                .ok_or_else(|| {
                    ProxyError::InvalidRequest("Grok auth JSON is required".to_string())
                }),
        })
    }

    fn persisted_provider_config(&self) -> Result<Option<ProviderConfig>, ProxyError> {
        self.provider_type().and_then(|kind| match kind {
            ProviderType::Chat => Ok(Some(ProviderConfig::OpenAiChat(
                crate::config::OpenAiChatConfig { base: self.base() },
            ))),
            ProviderType::Responses => Ok(Some(ProviderConfig::OpenAiResponses(
                crate::config::OpenAiResponsesConfig { base: self.base() },
            ))),
            ProviderType::Claude => Ok(Some(ProviderConfig::Claude(crate::config::ClaudeConfig {
                base: self.base(),
            }))),
            ProviderType::Gemini => Ok(Some(ProviderConfig::Gemini(crate::config::GeminiConfig {
                base: self.base(),
            }))),
            ProviderType::Codex => self
                .codex_config()
                .map(|config| config.map(ProviderConfig::Codex)),
            ProviderType::Grok => self
                .grok_config()
                .map(|config| config.map(ProviderConfig::Grok)),
        })
    }

    fn codex_config(&self) -> Result<Option<crate::config::CodexConfig>, ProxyError> {
        let auth_config = match self.auth.as_ref() {
            Some(auth_json) => parse_auth_value(auth_json.clone())?,
            None if self.api_key.trim().is_empty() => return Ok(None),
            None => OneOrMany::Many(Vec::new()),
        };
        Ok(Some(crate::config::CodexConfig {
            base: self.base(),
            auth: auth_config,
        }))
    }

    fn grok_config(&self) -> Result<Option<crate::config::GrokConfig>, ProxyError> {
        let auth_config = match self.auth.as_ref() {
            Some(auth_json) => parse_auth_value(auth_json.clone())?,
            None if self.api_key.trim().is_empty() => return Ok(None),
            None => OneOrMany::Many(Vec::new()),
        };
        Ok(Some(crate::config::GrokConfig {
            base: self.base(),
            auth: auth_config,
        }))
    }
}

fn parse_auth_value<T>(value: Value) -> Result<OneOrMany<T>, ProxyError>
where
    T: serde::de::DeserializeOwned,
{
    if value.is_array() {
        serde_json::from_value::<Vec<T>>(value)
            .map(OneOrMany::Many)
            .map_err(ProxyError::from)
    } else if value.is_object() {
        serde_json::from_value::<T>(value)
            .map(OneOrMany::One)
            .map_err(ProxyError::from)
    } else {
        Err(ProxyError::InvalidRequest(
            "auth must be a JSON object or array".to_string(),
        ))
    }
}
