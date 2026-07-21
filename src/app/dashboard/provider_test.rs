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
    provider::{UpstreamResponse, oauth, types::{HeaderMap, ProviderType}},
    state::AppState,
};

use super::auth_helpers::auth_headers;
use super::endpoints::build_provider_responses_endpoint;
use super::types::{
    DashboardProvider, ProviderTestRequest, ProviderTestResult, ProviderTestStreamResponse,
};

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

pub(super) fn write_provider_test_stream_response(res: &mut Response, response: ProviderTestStreamResponse) {
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

