use std::collections::HashMap;

use bytes::Bytes;
use serde_json::Value;
use tokio::time::{Duration, sleep};

use crate::{
    error::ProxyError,
    middleware::headers::{apply_map_headers, merge_headers},
    provider::{
        Providers, SendRequest, UpstreamResponse,
        types::{AttemptTarget, ProviderType},
    },
    retry::{backoff_delay_ms, fallback_chain},
    state::{AppSnapshot, AppState, AuthCursorKey, ProviderCursor},
};

#[derive(Debug, Clone)]
pub struct ExecuteRequest {
    pub target: ProviderType,
    pub model: String,
    pub is_streaming: bool,
    pub body: Value,
    pub forwarded_headers: crate::provider::types::HeaderMap,
}

#[derive(Debug)]
pub struct ExecuteResult {
    pub provider_type: ProviderType,
    pub response: UpstreamResponse,
}

#[derive(Debug, Clone)]
pub struct ExecuteImageRequest {
    pub model: String,
    pub body: Value,
    pub forwarded_headers: crate::provider::types::HeaderMap,
}

pub async fn execute(
    state: &AppState,
    snapshot: &AppSnapshot,
    request: ExecuteRequest,
) -> Result<ExecuteResult, ProxyError> {
    let resolved_model = snapshot.config.resolve_model_alias(&request.model);
    if resolved_model != request.model {
        tracing::info!(
            requested_model = %request.model,
            resolved_model = %resolved_model,
            "resolved model alias"
        );
    }
    let mut models_to_try = vec![resolved_model.clone()];
    models_to_try.extend(
        fallback_chain(&resolved_model, &snapshot.config)?
            .into_iter()
            .map(|model| snapshot.config.resolve_model_alias(&model)),
    );

    if !models_to_try
        .iter()
        .any(|model| snapshot.registry.has_any_provider_for_model(model))
    {
        return Err(ProxyError::ModelNotConfigured {
            model: request.model,
            attempted: models_to_try,
        });
    }

    let mut retry_budget = 0usize;
    let mut last_result: Option<ExecuteResult> = None;
    let mut last_error: Option<ProxyError> = None;

    for (model_index, model) in models_to_try.iter().enumerate() {
        if model_index > 0 {
            tracing::info!(
                previous_model = %models_to_try[model_index - 1],
                fallback_model = %model,
                "trying fallback model"
            );
        }

        if let Some(result) = execute_single_model(
            state,
            snapshot,
            &request,
            model,
            &mut retry_budget,
            &mut last_error,
        )
        .await?
        {
            if result.response.is_success() {
                return Ok(result);
            }
            last_result = Some(result);
        }
    }

    if let Some(result) = last_result {
        return Ok(result);
    }
    if let Some(error) = last_error {
        return Err(error);
    }
    Err(ProxyError::AllProvidersExhausted)
}

pub async fn execute_image(
    state: &AppState,
    snapshot: &AppSnapshot,
    request: ExecuteImageRequest,
) -> Result<ExecuteResult, ProxyError> {
    let resolved_model = snapshot.config.resolve_model_alias(&request.model);
    let mut targets = snapshot.registry.attempt_targets(&resolved_model);
    targets.retain(|target| image_provider_config(target).is_some());

    if targets.is_empty() {
        return Err(ProxyError::ModelNotConfigured {
            model: request.model.clone(),
            attempted: vec![resolved_model],
        });
    }

    rotate_attempt_targets(&mut targets, state.provider_cursor(&request.model).await);
    let mut retry_budget = 0usize;
    let mut last_result: Option<ExecuteResult> = None;
    let mut last_error: Option<ProxyError> = None;
    let mut target_attempts: HashMap<ProviderCursor, usize> = HashMap::new();

    loop {
        let mut found_valid_provider = false;

        for target in &targets {
            if !should_attempt(retry_budget, snapshot.config.retry.max_retries) {
                break;
            }

            found_valid_provider = true;
            retry_budget += 1;
            let attempt = retry_budget;
            let provider_cursor = provider_cursor(target);
            let target_attempt = target_attempts.entry(provider_cursor.clone()).or_insert(0);
            *target_attempt += 1;
            tracing::info!(
                model = %request.model,
                provider = ?target.provider_type,
                provider_index = target.provider_index,
                config_index = target.config_index,
                base_url = config_base_url(&target.config),
                attempt,
                provider_attempt = *target_attempt,
                max_retries = snapshot.config.retry.max_retries,
                "image provider attempt"
            );

            match try_image_target(state, &request, target).await {
                Ok(result) if result.response.is_success() => {
                    state
                        .record_provider_success(&request.model, provider_cursor)
                        .await;
                    return Ok(result);
                }
                Ok(result) => {
                    tracing::warn!(
                        model = %request.model,
                        provider = ?target.provider_type,
                        provider_index = target.provider_index,
                        config_index = target.config_index,
                        base_url = config_base_url(&target.config),
                        status_code = result.response.status(),
                        response_body = %result.response.body_text().unwrap_or_default(),
                        "image provider returned non-success"
                    );
                    last_result = Some(result);
                }
                Err(error) => {
                    tracing::warn!(
                        model = %request.model,
                        provider = ?target.provider_type,
                        provider_index = target.provider_index,
                        config_index = target.config_index,
                        base_url = config_base_url(&target.config),
                        error = %error,
                        "image provider attempt failed"
                    );
                    last_error = Some(error);
                }
            }
        }

        if !found_valid_provider || retry_budget >= snapshot.config.retry.max_retries {
            break;
        }

        let delay_ms = backoff_delay_ms(retry_budget, &snapshot.config);
        tracing::info!(
            model = %request.model,
            retry_budget,
            delay_ms,
            "image retry backoff"
        );
        sleep(Duration::from_millis(delay_ms)).await;
    }

    if let Some(result) = last_result {
        return Ok(result);
    }
    if let Some(error) = last_error {
        return Err(error);
    }
    Err(ProxyError::AllProvidersExhausted)
}

async fn execute_single_model(
    state: &AppState,
    snapshot: &AppSnapshot,
    request: &ExecuteRequest,
    model: &str,
    retry_budget: &mut usize,
    last_error: &mut Option<ProxyError>,
) -> Result<Option<ExecuteResult>, ProxyError> {
    let mut targets = snapshot.registry.attempt_targets(model);
    rotate_attempt_targets(&mut targets, state.provider_cursor(model).await);
    let mut last_result = None;
    let mut target_attempts: HashMap<ProviderCursor, usize> = HashMap::new();

    loop {
        let mut found_valid_provider = false;
        for target in &targets {
            if !should_attempt(*retry_budget, snapshot.config.retry.max_retries) {
                break;
            }

            found_valid_provider = true;
            *retry_budget += 1;
            let attempt = *retry_budget;
            let provider_cursor = provider_cursor(target);
            let target_attempt = target_attempts.entry(provider_cursor.clone()).or_insert(0);
            *target_attempt += 1;
            let target_attempt = *target_attempt;
            let auth_key = auth_cursor_key(target);
            let auth_start_index = if let Some(key) = auth_key.as_ref() {
                state.auth_cursor(key).await
            } else {
                None
            };
            tracing::info!(
                model = %model,
                provider = ?target.provider_type,
                provider_index = target.provider_index,
                config_index = target.config_index,
                base_url = config_base_url(&target.config),
                attempt,
                provider_attempt = target_attempt,
                max_retries = snapshot.config.retry.max_retries,
                "provider attempt"
            );

            match try_target(
                &state.providers,
                state,
                request,
                model,
                target,
                auth_start_index,
                target_attempt,
            )
            .await
            {
                Ok(result) if result.response.is_success() => {
                    state.record_provider_success(model, provider_cursor).await;
                    if let (Some(key), Some(auth_index)) = (auth_key, result.response.auth_index())
                    {
                        state.record_auth_success(key, auth_index).await;
                    }
                    return Ok(Some(result));
                }
                Ok(result) => {
                    tracing::warn!(
                        model = %model,
                        provider = ?target.provider_type,
                        provider_index = target.provider_index,
                        config_index = target.config_index,
                        base_url = config_base_url(&target.config),
                        status_code = result.response.status(),
                        response_body = %result.response.body_text().unwrap_or_default(),
                        "provider returned non-success"
                    );
                    last_result = Some(result);
                }
                Err(error) => {
                    tracing::warn!(
                        model = %model,
                        provider = ?target.provider_type,
                        provider_index = target.provider_index,
                        config_index = target.config_index,
                        base_url = config_base_url(&target.config),
                        error = %error,
                        "provider attempt failed"
                    );
                    *last_error = Some(error);
                }
            }
        }

        if !found_valid_provider || *retry_budget >= snapshot.config.retry.max_retries {
            break;
        }

        let delay_ms = backoff_delay_ms(*retry_budget, &snapshot.config);
        tracing::info!(
            model = %model,
            retry_budget = *retry_budget,
            delay_ms,
            "retry backoff"
        );
        sleep(Duration::from_millis(delay_ms)).await;
    }

    Ok(last_result)
}

pub fn should_attempt(attempts: usize, max_retries: usize) -> bool {
    attempts < max_retries
}

async fn try_target(
    providers: &Providers,
    state: &AppState,
    request: &ExecuteRequest,
    model: &str,
    target: &AttemptTarget,
    auth_start_index: Option<usize>,
    target_attempt: usize,
) -> Result<ExecuteResult, ProxyError> {
    let body = request_with_model(request.body.clone(), model).map_err(|error| {
        tracing::debug!(
            model = %model,
            provider = ?target.provider_type,
            provider_index = target.provider_index,
            config_index = target.config_index,
            base_url = config_base_url(&target.config),
            error = %error,
            raw_request_body = %request.body,
            "request body normalization failed"
        );
        error
    })?;
    let provider_request = providers
        .prepare_request(
            target.provider_type,
            body.clone(),
            request.target,
            request.is_streaming,
        )
        .map_err(|error| {
            tracing::debug!(
                source_provider = ?request.target,
                target_provider = ?target.provider_type,
                model = %model,
                provider_index = target.provider_index,
                config_index = target.config_index,
                base_url = config_base_url(&target.config),
                error = %error,
                raw_request_body = %body,
                "request conversion failed"
            );
            error
        })?;
    let upstream = providers
        .send_request(SendRequest {
            state: Some(state),
            client: &state.http,
            is_streaming: request.is_streaming,
            provider_type: target.provider_type,
            request: provider_request,
            config: &target.config,
            config_index: target.config_index,
            forwarded_headers: &request.forwarded_headers,
            model,
            auth_start_index,
            target_attempt,
        })
        .await
        .map_err(|error| {
            let upstream_status_code = error.upstream_status_code().map(|status| status.as_u16());
            let upstream_url = error.upstream_url();
            tracing::warn!(
                model = %model,
                provider = ?target.provider_type,
                provider_index = target.provider_index,
                config_index = target.config_index,
                base_url = config_base_url(&target.config),
                status_code = error.status_code().as_u16(),
                upstream_status_code = ?upstream_status_code,
                upstream_url = upstream_url.as_deref().unwrap_or(""),
                error = %error,
                "upstream request failed"
            );
            error
        })?;

    Ok(ExecuteResult {
        provider_type: target.provider_type,
        response: upstream,
    })
}

fn config_base_url(config: &crate::config::ProviderConfig) -> &str {
    config.base_url().unwrap_or("<provider default>")
}

pub fn rotate_attempt_targets(targets: &mut [AttemptTarget], cursor: Option<ProviderCursor>) {
    let Some(cursor) = cursor else {
        return;
    };
    if let Some(index) = targets
        .iter()
        .position(|target| provider_cursor(target) == cursor)
    {
        targets.rotate_left(index);
    }
}

fn provider_cursor(target: &AttemptTarget) -> ProviderCursor {
    ProviderCursor {
        provider_type: target.provider_type,
        base_url: target.base_url(),
        config_index: target.config_index,
    }
}

fn auth_cursor_key(target: &AttemptTarget) -> Option<AuthCursorKey> {
    matches!(
        target.provider_type,
        ProviderType::Codex | ProviderType::Grok
    )
    .then_some(AuthCursorKey {
        provider_type: target.provider_type,
        base_url: target.base_url(),
        config_index: target.config_index,
    })
}

fn request_with_model(mut body: Value, model: &str) -> Result<Value, ProxyError> {
    let obj = body.as_object_mut().ok_or_else(|| {
        ProxyError::InvalidRequest("request body must be a JSON object".to_string())
    })?;
    obj.insert("model".to_string(), Value::String(model.to_string()));
    Ok(body)
}

pub fn bytes_to_json(bytes: Bytes) -> Result<Value, ProxyError> {
    serde_json::from_slice(&bytes).map_err(ProxyError::from)
}

async fn try_image_target(
    state: &AppState,
    request: &ExecuteImageRequest,
    target: &AttemptTarget,
) -> Result<ExecuteResult, ProxyError> {
    let Some(config) = image_provider_config(target) else {
        return Err(ProxyError::InvalidRequest(format!(
            "provider {:?} does not support image generation",
            target.provider_type
        )));
    };
    let mut headers = merge_headers(
        &request.forwarded_headers,
        &[
            ("content-type", "application/json".to_string()),
            ("authorization", format!("Bearer {}", config.api_key)),
        ],
    );
    apply_map_headers(&mut headers, config.headers);
    let response = state
        .http
        .post(format!(
            "{}/v1/images/generations",
            clean_image_base_url(config.base_url)
        ))
        .headers(super::reqwest_headers(&headers)?)
        .json(&request.body)
        .send()
        .await
        .map_err(|error| {
            let upstream_status_code = error.status().map(|status| status.as_u16());
            let upstream_url = error.url().map(|url| url.as_str().to_string());
            tracing::warn!(
                model = %request.model,
                provider = ?target.provider_type,
                provider_index = target.provider_index,
                config_index = target.config_index,
                base_url = %config.base_url,
                upstream_status_code = ?upstream_status_code,
                upstream_url = upstream_url.as_deref().unwrap_or(""),
                error = %error,
                "image upstream request failed"
            );
            error
        })?;
    let response = super::collect_response(response, false).await?;
    if let UpstreamResponse::NonStream { status, body, .. } = &response {
        tracing::debug!(
            provider = ?target.provider_type,
            model = %request.model,
            status_code = status,
            raw_response_body = %String::from_utf8_lossy(body),
            "image upstream raw response body"
        );
    }
    Ok(ExecuteResult {
        provider_type: target.provider_type,
        response,
    })
}

pub struct ImageProviderConfig<'a> {
    pub base_url: &'a str,
    pub api_key: &'a str,
    pub headers: &'a HashMap<String, String>,
}

pub fn image_provider_config(target: &AttemptTarget) -> Option<ImageProviderConfig<'_>> {
    match &target.config {
        crate::config::ProviderConfig::OpenAiChat(config) => Some(ImageProviderConfig {
            base_url: &config.base.base_url,
            api_key: &config.base.api_key,
            headers: &config.base.headers,
        }),
        crate::config::ProviderConfig::OpenAiResponses(config) => Some(ImageProviderConfig {
            base_url: &config.base.base_url,
            api_key: &config.base.api_key,
            headers: &config.base.headers,
        }),
        crate::config::ProviderConfig::Claude(config) => Some(ImageProviderConfig {
            base_url: &config.base.base_url,
            api_key: &config.base.api_key,
            headers: &config.base.headers,
        }),
        crate::config::ProviderConfig::Gemini(config) => Some(ImageProviderConfig {
            base_url: &config.base.base_url,
            api_key: &config.base.api_key,
            headers: &config.base.headers,
        }),
        crate::config::ProviderConfig::Codex(_) | crate::config::ProviderConfig::Grok(_) => None,
    }
}

pub fn clean_image_base_url(url: &str) -> String {
    url.trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or_else(|| url.trim_end_matches('/'))
        .trim_end_matches('/')
        .to_string()
}
