use std::{collections::HashSet, time::Duration};

use serde_json::{Value, json};

use crate::{
    error::ProxyError,
    provider::{oauth, types::ProviderType},
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

pub(super) async fn validate_auths(
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
            // 2xx: auth is usable; skip response body to keep the payload small.
            if (200..300).contains(&status_code) {
                let _ = response.bytes().await;
                return AuthValidationProbe {
                    valid: true,
                    reason: "ok".to_string(),
                    status_code,
                    error_message: String::new(),
                };
            }

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
        200..=299 => (true, "ok"),
        402 => (false, "payment_required"),
        403 => (false, "forbidden"),
        429 => (true, "rate_limited"),
        400..=499 => (true, "request_error"),
        500..=599 => (true, "server_error"),
        _ => (false, "unexpected"),
    };
    // Only non-2xx responses include upstream body/error details.
    let error_message = if (200..300).contains(&status_code) {
        String::new()
    } else {
        validation_error_message(error_type.as_ref(), body)
    };
    AuthValidationProbe {
        valid,
        reason: reason.to_string(),
        status_code,
        error_message,
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
