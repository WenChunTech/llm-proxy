use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures_util::stream::{self, StreamExt};
use serde_json::{Value, json};

use crate::{
    error::ProxyError,
    middleware::headers::{apply_map_headers, apply_optional_map_headers, merge_headers},
    provider::{
        oauth,
        types::{HeaderMap, ProviderType},
    },
    state::AppState,
    util::{auth_disabled, auth_string, set_auth_bool},
};

use super::endpoints::build_provider_responses_endpoint;
use super::types::{
    AuthValidatePayload, AuthValidateRequest, AuthValidateResponse, AuthValidateResult,
    DashboardAuthConfig, DashboardAuthProvider,
};

const CODEX_VALIDATION_MODEL: &str = "gpt-5.4";
const GROK_VALIDATION_MODEL: &str = "grok-4.5";
const DEFAULT_AUTH_VALIDATION_CONCURRENCY: usize = 5;

/// Progressive validation events for WebSocket clients.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum AuthValidateStreamEvent {
    Started {
        kind: String,
        model: String,
        total: usize,
    },
    Result {
        completed: usize,
        total: usize,
        result: AuthValidateResult,
    },
    Done {
        success: bool,
        data: AuthValidatePayload,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone)]
struct AuthValidationProbe {
    valid: bool,
    reason: String,
    status_code: u16,
    error_message: String,
    curl: String,
}

impl AuthValidationProbe {
    fn skipped(reason: &str, message: impl Into<String>) -> Self {
        Self {
            valid: false,
            reason: reason.to_string(),
            status_code: 0,
            error_message: message.into(),
            curl: String::new(),
        }
    }
}

pub(super) async fn validate_auths(
    state: &AppState,
    request: AuthValidateRequest,
    provider_type: ProviderType,
) -> Result<AuthValidateResponse, ProxyError> {
    validate_auths_with_progress(state, request, provider_type, None).await
}

pub(super) async fn validate_auths_with_progress(
    state: &AppState,
    request: AuthValidateRequest,
    provider_type: ProviderType,
    progress_tx: Option<tokio::sync::mpsc::Sender<AuthValidateStreamEvent>>,
) -> Result<AuthValidateResponse, ProxyError> {
    let kind = provider_type.as_str().to_string();
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

    // Pre-count / build units of work so clients can render progress immediately
    // and so validation can run with bounded concurrency.
    let mut planned_tasks: Vec<AuthValidationTask> = Vec::new();
    for provider_index in &provider_indices {
        let Some(provider) = providers.get(*provider_index) else {
            continue;
        };
        let auth_items = auth_validation_items(provider.auth.as_ref());
        let auth_count = auth_items.len();
        let is_auth_array = provider.auth.as_ref().is_some_and(Value::is_array);
        if auth_items.is_empty() {
            planned_tasks.push(AuthValidationTask {
                provider_index: *provider_index,
                auth_index: 0,
                auth_count: 0,
                is_auth_array,
                label: auth_validation_label(provider_type, *provider_index, 0, 0, None),
                provider_enabled: provider.enabled,
                auth: Value::Null,
                empty_auth: true,
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
            planned_tasks.push(AuthValidationTask {
                provider_index: *provider_index,
                auth_index,
                auth_count,
                is_auth_array,
                label: auth_validation_label(
                    provider_type,
                    *provider_index,
                    auth_index,
                    auth_count,
                    Some(auth),
                ),
                provider_enabled: provider.enabled,
                auth: auth.clone(),
                empty_auth: false,
            });
        }
    }

    let planned = planned_tasks.len();
    let concurrency = resolve_auth_validation_concurrency(request.concurrency);

    emit_progress(
        &progress_tx,
        AuthValidateStreamEvent::Started {
            kind: kind.clone(),
            model: model.clone(),
            total: planned,
        },
    )
    .await;

    let providers = Arc::new(providers);
    let probe_model = Arc::new(model.clone());
    let completed = Arc::new(AtomicUsize::new(0));
    let http = state.http.clone();

    let mut results: Vec<AuthValidateResult> = stream::iter(planned_tasks)
        .map(|task| {
            let providers = Arc::clone(&providers);
            let probe_model = Arc::clone(&probe_model);
            let progress_tx = progress_tx.clone();
            let completed = Arc::clone(&completed);
            let http = http.clone();
            async move {
                let result = if task.empty_auth {
                    AuthValidateResult {
                        provider_index: task.provider_index,
                        auth_index: task.auth_index,
                        auth_count: task.auth_count,
                        is_auth_array: task.is_auth_array,
                        label: task.label,
                        disabled: !task.provider_enabled,
                        skipped: true,
                        valid: false,
                        reason: "no_auth".to_string(),
                        status_code: 0,
                        error_message: "auth JSON is empty".to_string(),
                        refreshed: false,
                        auth: Value::Null,
                        curl: String::new(),
                    }
                } else {
                    let Some(provider) = providers.get(task.provider_index) else {
                        return AuthValidateResult {
                            provider_index: task.provider_index,
                            auth_index: task.auth_index,
                            auth_count: task.auth_count,
                            is_auth_array: task.is_auth_array,
                            label: task.label,
                            disabled: !task.provider_enabled,
                            skipped: true,
                            valid: false,
                            reason: "no_auth".to_string(),
                            status_code: 0,
                            error_message: "provider is missing".to_string(),
                            refreshed: false,
                            auth: Value::Null,
                            curl: String::new(),
                        };
                    };
                    let mut auth = task.auth;
                    let disabled = auth_disabled(&auth) || !task.provider_enabled;
                    let (probe, refreshed) = validate_single_auth(
                        &http,
                        provider_type,
                        provider,
                        &mut auth,
                        probe_model.as_str(),
                    )
                    .await;
                    if probe.reason == "rate_limited" {
                        set_auth_bool(&mut auth, "disabled", true);
                    }
                    AuthValidateResult {
                        provider_index: task.provider_index,
                        auth_index: task.auth_index,
                        auth_count: task.auth_count,
                        is_auth_array: task.is_auth_array,
                        label: task.label,
                        disabled: auth_disabled(&auth) || disabled,
                        skipped: false,
                        valid: probe.valid,
                        reason: probe.reason,
                        status_code: probe.status_code,
                        error_message: probe.error_message,
                        refreshed,
                        auth,
                        curl: probe.curl,
                    }
                };

                let completed_count = completed.fetch_add(1, Ordering::Relaxed) + 1;
                emit_progress(
                    &progress_tx,
                    AuthValidateStreamEvent::Result {
                        completed: completed_count,
                        total: planned,
                        result: result.clone(),
                    },
                )
                .await;
                result
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    results.sort_by(|a, b| {
        a.provider_index
            .cmp(&b.provider_index)
            .then(a.auth_index.cmp(&b.auth_index))
    });

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

    let data = AuthValidatePayload {
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
    };
    emit_progress(
        &progress_tx,
        AuthValidateStreamEvent::Done {
            success: true,
            data: data.clone(),
        },
    )
    .await;

    Ok(AuthValidateResponse {
        success: true,
        data,
    })
}

#[derive(Debug, Clone)]
struct AuthValidationTask {
    provider_index: usize,
    auth_index: usize,
    auth_count: usize,
    is_auth_array: bool,
    label: String,
    provider_enabled: bool,
    auth: Value,
    empty_auth: bool,
}

pub fn resolve_auth_validation_concurrency(requested: Option<usize>) -> usize {
    requested
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_AUTH_VALIDATION_CONCURRENCY)
}

async fn emit_progress(
    progress_tx: &Option<tokio::sync::mpsc::Sender<AuthValidateStreamEvent>>,
    event: AuthValidateStreamEvent,
) {
    if let Some(tx) = progress_tx {
        let _ = tx.send(event).await;
    }
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
                headers: provider.base.headers.clone(),
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
                headers: provider.base.headers.clone(),
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

    if let Some(token) = auth_string(auth, "access_token") {
        let should_refresh = oauth::should_refresh_value(provider_type, auth);
        let probe =
            probe_validation_auth(client, provider_type, provider, auth, &token, model).await;
        let should_try_refresh = auth_string(auth, "refresh_token").is_some()
            && (probe.reason == "invalid_auth" || should_refresh);
        if should_try_refresh {
            match refresh_validation_auth(client, provider_type, auth).await {
                Ok(()) => {
                    let Some(token) = auth_string(auth, "access_token") else {
                        return (
                            AuthValidationProbe::skipped(
                                "missing_access_token",
                                "refresh did not return access token",
                            ),
                            true,
                        );
                    };
                    // Re-probe with the refreshed token so the result matches real curl.
                    let new_probe =
                        probe_validation_auth(client, provider_type, provider, auth, &token, model)
                            .await;
                    return (new_probe, true);
                }
                Err(error) => {
                    tracing::warn!(
                        provider = ?provider_type,
                        error = %error,
                        "auth validation token refresh failed after auth probe"
                    );
                }
            }
        }
        return (probe, false);
    }

    if auth_string(auth, "refresh_token").is_none() {
        return (
            AuthValidationProbe::skipped(
                "missing_access_token",
                "access_token is missing and refresh_token is not configured",
            ),
            false,
        );
    }

    let refreshed = match refresh_validation_auth(client, provider_type, auth).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(
                provider = ?provider_type,
                error = %error,
                "auth validation token refresh failed before auth probe"
            );
            return (
                AuthValidationProbe::skipped(
                    "missing_access_token",
                    "access_token is missing and refresh_token could not refresh an access token",
                ),
                false,
            );
        }
    };

    let Some(token) = auth_string(auth, "access_token") else {
        return (
            AuthValidationProbe::skipped(
                "missing_access_token",
                "refresh_token refresh did not return an access_token",
            ),
            refreshed,
        );
    };

    (
        probe_validation_auth(client, provider_type, provider, auth, &token, model).await,
        refreshed,
    )
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
    let prompt = "hello";
    let body = match provider_type {
        ProviderType::Codex => json!({
            "model": model,
            "input": prompt,
            "stream": true,
            "max_output_tokens": 16,
            "store": false,
            "instructions": "",
        }),
        ProviderType::Grok => json!({
            "model": model,
            "input": prompt,
            "stream": true,
            "max_output_tokens": 16,
        }),
        _ => unreachable!("auth validation is only supported for Codex and Grok"),
    };
    // Match formal Codex/Grok proxy construction:
    // fixed protocol headers -> auth.headers -> provider.headers, then .json().
    // Important: set headers *before* `.json()`. reqwest's `.json()` only inserts
    // Content-Type when missing (`or_insert`). Applying headers after `.json()` via
    // `.header()` *appends* a second Content-Type; xAI/Codex reject dual
    // Content-Type with HTTP 415, while the generated single-header curl succeeds.
    let headers = validation_request_headers(provider_type, token, auth, &provider.headers);
    let curl = validation_request_curl(&endpoint, &headers, &body);
    let builder = build_validation_probe_request(client, &endpoint, &headers, &body);

    match builder.send().await {
        Ok(response) => {
            let status_code = response.status().as_u16();
            // 2xx: auth is usable; skip response body to keep the payload small.
            if (200..300).contains(&status_code) {
                let _ = response.bytes().await;
                return AuthValidationProbe {
                    valid: true,
                    reason: "ok".to_string(),
                    status_code,
                    error_message: String::new(),
                    curl,
                };
            }

            let body = response.text().await.unwrap_or_default();
            classify_validation_response(provider_type, status_code, &body, curl)
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
                curl,
            }
        }
    }
}

fn classify_validation_response(
    provider_type: ProviderType,
    status_code: u16,
    body: &str,
    curl: String,
) -> AuthValidationProbe {
    let (valid, reason, error_message) =
        classify_auth_validation_response(provider_type, status_code, body);
    AuthValidationProbe {
        valid,
        reason,
        status_code,
        error_message,
        curl,
    }
}

/// Classify an upstream validation response into valid/reason/message.
///
/// `error_message` is the **raw upstream body** (unchanged) so the dashboard can
/// show exactly what curl would print. Classification still parses the body
/// internally; display must not use extracted/summarized fields.
/// Exposed for unit tests so Codex/Grok auth failure shapes stay covered.
pub fn classify_auth_validation_response(
    provider_type: ProviderType,
    status_code: u16,
    body: &str,
) -> (bool, String, String) {
    let error_details = extract_validation_error_details(body);
    let lower_body = body.to_lowercase();
    // Always surface the upstream body verbatim for non-2xx probes.
    let raw_body = body.to_string();
    if is_invalid_auth_response(provider_type, status_code, &error_details, &lower_body) {
        return (false, "invalid_auth".to_string(), raw_body);
    }

    let (valid, reason) = match status_code {
        200..=299 => (true, "ok"),
        402 => (false, "payment_required"),
        403 => (false, "forbidden"),
        429 => (true, "rate_limited"),
        400..=499 => (true, "request_error"),
        500..=599 => (true, "server_error"),
        _ => (false, "unexpected"),
    };
    let error_message = if (200..300).contains(&status_code) {
        String::new()
    } else {
        raw_body
    };
    (valid, reason.to_string(), error_message)
}

/// Build the validation probe RequestBuilder the same way as production so tests
/// can assert a single Content-Type (xAI rejects dual Content-Type with 415).
pub fn build_validation_probe_request(
    client: &reqwest::Client,
    endpoint: &str,
    headers: &HeaderMap,
    body: &Value,
) -> reqwest::RequestBuilder {
    let mut builder = client.post(endpoint).timeout(Duration::from_secs(30));
    for (name, value) in headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    // Headers first, then `.json()`: reqwest only inserts Content-Type when absent.
    builder.json(body)
}

/// Parse upstream error payloads for both nested OpenAI-style objects and flat
/// xAI-style `{"code":"...","error":"..."}` responses.
fn extract_validation_error_details(body: &str) -> Option<(String, String, String)> {
    let value = serde_json::from_str::<Value>(body).ok()?;

    if let Some(error) = value.get("error").filter(|error| error.is_object()) {
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
        return Some((error_type, error_code, message));
    }

    // Flat shape used by xAI OAuth failures:
    // {"code":"unauthenticated:bad-credentials","error":"The OAuth2 access token could not be validated."}
    let error_code = value
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let message = value
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();
    if error_code.is_empty() && message.is_empty() {
        return None;
    }
    Some((String::new(), error_code, message))
}

fn is_invalid_auth_response(
    provider_type: ProviderType,
    status_code: u16,
    error_details: &Option<(String, String, String)>,
    lower_body: &str,
) -> bool {
    if status_code == 401 {
        return true;
    }

    if error_details
        .as_ref()
        .is_some_and(|(error_type, error_code, message)| {
            auth_failure_token(error_type)
                || auth_failure_token(error_code)
                || auth_failure_token(message)
        })
    {
        return true;
    }

    if auth_failure_token(lower_body) {
        return true;
    }

    // 403 from OAuth gateways is often an auth failure rather than a resource ACL.
    if status_code == 403
        && (provider_type == ProviderType::Grok || provider_type == ProviderType::Codex)
        && (lower_body.contains("unauthenticated")
            || lower_body.contains("token")
            || lower_body.contains("credential")
            || lower_body.contains("oauth"))
    {
        return true;
    }

    false
}

fn auth_failure_token(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("authentication_error")
        || lower.contains("invalid_api_key")
        || lower.contains("invalid or expired token")
        || lower.contains("access token could not be validated")
        || lower.contains("unauthenticated")
        || lower.contains("bad-credentials")
        || lower.contains("bad_credentials")
        || lower.contains("unauthorized")
        || lower.contains("invalid_token")
        || lower.contains("token_expired")
        || lower.contains("expired_token")
}

fn validation_request_curl(endpoint: &str, headers: &HeaderMap, body: &Value) -> String {
    let header_args = headers
        .iter()
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(name, value)| format!("-H {}", shell_quote(&format!("{name}: {value}"))))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "curl -sS -X POST {} {} --data-raw {}",
        shell_quote(endpoint),
        header_args,
        shell_quote(&body.to_string()),
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Build headers for Codex/Grok auth validation using the same merge rules as
/// the formal provider request path.
pub fn validation_request_headers(
    provider_type: ProviderType,
    token: &str,
    auth: &Value,
    provider_headers: &std::collections::HashMap<String, String>,
) -> HeaderMap {
    let forwarded = HeaderMap::new();
    let mut fixed = vec![
        ("content-type", "application/json".to_string()),
        ("authorization", format!("Bearer {token}")),
        ("accept", "text/event-stream".to_string()),
    ];
    if provider_type == ProviderType::Codex {
        fixed.extend([
            ("user-agent", oauth::CODEX_USER_AGENT.to_string()),
            ("originator", "codex-tui".to_string()),
            ("connection", "Keep-Alive".to_string()),
        ]);
    }

    let mut headers = merge_headers(&forwarded, &fixed);
    if provider_type == ProviderType::Codex
        && let Some(account_id) = auth
            .get("account_id")
            .or_else(|| auth.get("chatgpt_account_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        headers.insert("chatgpt-account-id".to_string(), account_id.to_string());
    }

    // Priority: custom provider headers override auth headers on conflicts.
    apply_optional_map_headers(&mut headers, auth_headers_map(auth).as_ref());
    apply_map_headers(&mut headers, provider_headers);
    headers
}

fn auth_headers_map(auth: &Value) -> Option<std::collections::HashMap<String, String>> {
    let headers = auth.get("headers")?.as_object()?;
    let mut out = std::collections::HashMap::new();
    for (name, value) in headers {
        let Some(value) = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        out.insert(name.clone(), value.to_string());
    }
    Some(out)
}

pub fn validation_auth_base_url<'a>(
    _provider_type: ProviderType,
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

    (!provider_base_url.is_empty())
        .then_some(provider_base_url)
        .or(auth_base_url)
        .unwrap_or(default_base_url)
}

async fn refresh_validation_auth(
    client: &reqwest::Client,
    provider_type: ProviderType,
    auth: &mut Value,
) -> Result<(), ProxyError> {
    match provider_type {
        ProviderType::Codex => oauth::refresh_codex_auth_value(client, auth).await,
        ProviderType::Grok => oauth::refresh_grok_auth_value(client, auth).await,
        _ => Ok(()),
    }
}
