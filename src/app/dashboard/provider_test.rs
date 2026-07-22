use std::time::Duration;

use bytes::Bytes;
use futures_util::TryStreamExt;
use salvo::{
    http::{HeaderValue, StatusCode, header},
    prelude::*,
};
use serde_json::{Value, json};

use crate::{
    app::apply_headers,
    error::ProxyError,
    provider::{
        UpstreamResponse,
        credentials::credential_slot_count,
        types::{AttemptTarget, HeaderMap, ProviderType},
    },
    state::AppState,
};

use super::types::{ProviderTestRequest, ProviderTestResult, ProviderTestStreamResponse};

pub(super) async fn test_provider_model(
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
    let config = request.provider.provider_config()?;
    let attempt_count = provider_test_attempt_count(provider_type, &config);
    let mut last_result = None;
    let mut last_error = None;

    for target_attempt in 1..=attempt_count {
        match send_provider_test_attempt(
            state,
            provider_type,
            &config,
            provider_request.clone(),
            model,
            stream,
            target_attempt,
        )
        .await
        {
            Ok(upstream) => {
                let status = upstream.status();
                let body = collect_test_response_body(upstream).await?;
                let raw_body = response_text(&body);
                let body_preview = response_preview(&body);
                let result = ProviderTestResult {
                    ok: (200..300).contains(&status),
                    status,
                    provider: request.provider.kind.clone(),
                    model: model.to_string(),
                    stream,
                    raw_body,
                    body_preview,
                };
                if result.ok {
                    return Ok(result);
                }
                last_result = Some(result);
            }
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    if let Some(result) = last_result {
        return Ok(result);
    }
    Err(last_error.unwrap_or(ProxyError::AllProvidersExhausted))
}

pub(super) async fn stream_provider_model(
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

    let config = request.provider.provider_config()?;
    let attempt_count = provider_test_attempt_count(provider_type, &config);
    let mut last_response = None;
    let mut last_error = None;

    for target_attempt in 1..=attempt_count {
        match send_provider_test_attempt(
            state,
            provider_type,
            &config,
            provider_request.clone(),
            model,
            stream,
            target_attempt,
        )
        .await
        {
            Ok(upstream) if upstream.is_success() => {
                return Ok(ProviderTestStreamResponse::Upstream(upstream));
            }
            Ok(upstream) => {
                last_response = Some(upstream);
            }
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    if let Some(response) = last_response {
        return Ok(ProviderTestStreamResponse::Upstream(response));
    }
    Err(last_error.unwrap_or(ProxyError::AllProvidersExhausted))
}

async fn send_provider_test_attempt(
    state: &AppState,
    provider_type: ProviderType,
    config: &crate::config::ProviderConfig,
    provider_request: Value,
    model: &str,
    stream: bool,
    target_attempt: usize,
) -> Result<UpstreamResponse, ProxyError> {
    let headers = HeaderMap::new();
    tokio::time::timeout(
        Duration::from_secs(30),
        state.providers.send_request(crate::provider::SendRequest {
            state: None,
            client: &state.http,
            is_streaming: stream,
            provider_type,
            request: provider_request,
            config,
            config_index: 0,
            forwarded_headers: &headers,
            model,
            auth_start_index: None,
            target_attempt,
        }),
    )
    .await
    .map_err(|_| ProxyError::Upstream("provider test timed out".to_string()))?
}

fn provider_test_attempt_count(
    provider_type: ProviderType,
    config: &crate::config::ProviderConfig,
) -> usize {
    let target = AttemptTarget {
        provider_type,
        provider_index: 0,
        config_index: 0,
        config: config.clone(),
    };
    credential_slot_count(&target)
}

pub(super) fn write_provider_test_stream_response(
    res: &mut Response,
    response: ProviderTestStreamResponse,
) {
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
    }
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

fn response_preview(body: &[u8]) -> String {
    let text = response_text(body);
    text.chars().take(500).collect()
}
