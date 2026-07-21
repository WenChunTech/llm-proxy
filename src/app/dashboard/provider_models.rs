use std::time::Duration;

use serde_json::Value;

use crate::{error::ProxyError, provider::oauth};

use super::auth_helpers::{auth_access_token, auth_headers};
use super::endpoints::build_provider_models_endpoint;
use super::types::{ProviderModelsRequest, ProviderModelsResult};

pub(super) async fn fetch_provider_models(
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
