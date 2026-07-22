use std::collections::HashMap;

use bytes::Bytes;
use serde_json::Value;
use tokio::time::{Duration, sleep};

pub use crate::retry::should_attempt;

use crate::{
    error::ProxyError,
    middleware::headers::{apply_map_headers, merge_headers},
    provider::{
        Providers, SendRequest, UpstreamResponse,
        credentials::{
            coverage_attempt_budget, credential_coverage_attempts, credential_slot_count,
        },
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

        // Each model gets an independent attempt budget so fallback models are not
        // starved after the primary model exhausts its coverage/max_retries.
        if let Some(result) =
            execute_single_model(state, snapshot, &request, model, &mut last_error).await?
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
    match run_provider_attempts(
        &request.model,
        snapshot.config.retry.max_retries,
        &snapshot.config,
        &targets,
        &mut retry_budget,
        "image",
        |target, _target_attempt| {
            let request = request.clone();
            let target = target.clone();
            async move {
                let result = try_image_target(state, &request, &target).await?;
                if result.response.is_success() {
                    state
                        .record_provider_success(&request.model, provider_cursor(&target))
                        .await;
                }
                Ok(result)
            }
        },
    )
    .await?
    {
        AttemptLoopResult::Success(result) => Ok(result),
        AttemptLoopResult::Exhausted {
            last_result: Some(result),
            ..
        } => Ok(result),
        AttemptLoopResult::Exhausted {
            last_error: Some(error),
            ..
        } => Err(error),
        AttemptLoopResult::Exhausted { .. } => Err(ProxyError::AllProvidersExhausted),
    }
}

#[derive(Debug)]
enum AttemptLoopResult {
    Success(ExecuteResult),
    Exhausted {
        last_result: Option<ExecuteResult>,
        last_error: Option<ProxyError>,
    },
}

async fn run_provider_attempts<F, Fut>(
    model: &str,
    max_retries: usize,
    backoff_config: &crate::config::Config,
    targets: &[AttemptTarget],
    retry_budget: &mut usize,
    log_label: &str,
    mut attempt: F,
) -> Result<AttemptLoopResult, ProxyError>
where
    F: FnMut(&AttemptTarget, usize) -> Fut,
    Fut: std::future::Future<Output = Result<ExecuteResult, ProxyError>>,
{
    let mut last_result = None;
    let mut last_error = None;
    let mut target_attempts: HashMap<ProviderCursor, usize> = HashMap::new();
    // Shared attempt loop for every provider type (OpenAI/Claude/Gemini/Codex/Grok).
    // Budget always covers every target/credential once; max_retries only raises it.
    let coverage_attempts = credential_coverage_attempts(targets);
    let attempt_limit = coverage_attempt_budget(targets, max_retries);
    let mut round_index = 0usize;

    loop {
        let mut found_valid_provider = false;
        for target in targets {
            if !should_attempt(*retry_budget, attempt_limit) {
                break;
            }
            if *retry_budget < coverage_attempts && round_index >= credential_slot_count(target) {
                continue;
            }

            found_valid_provider = true;
            *retry_budget += 1;
            let attempt_no = *retry_budget;
            let cursor = provider_cursor(target);
            let target_attempt = target_attempts.entry(cursor.clone()).or_insert(0);
            *target_attempt += 1;
            let target_attempt = *target_attempt;

            tracing::info!(
                model = %model,
                provider = ?target.provider_type,
                provider_index = target.provider_index,
                config_index = target.config_index,
                base_url = config_base_url(&target.config),
                attempt = attempt_no,
                provider_attempt = target_attempt,
                max_retries,
                coverage_attempts,
                attempt_limit,
                "{log_label} provider attempt"
            );

            match attempt(target, target_attempt).await {
                Ok(result) if result.response.is_success() => {
                    return Ok(AttemptLoopResult::Success(result));
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
                        "{log_label} provider returned non-success"
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
                        "{log_label} provider attempt failed"
                    );
                    last_error = Some(error);
                }
            }
        }

        if !found_valid_provider || *retry_budget >= attempt_limit {
            break;
        }

        round_index = round_index.saturating_add(1);
        let delay_ms = backoff_delay_ms(*retry_budget, backoff_config);
        tracing::info!(
            model = %model,
            retry_budget = *retry_budget,
            delay_ms,
            "{log_label} retry backoff"
        );
        sleep(Duration::from_millis(delay_ms)).await;
    }

    Ok(AttemptLoopResult::Exhausted {
        last_result,
        last_error,
    })
}

async fn execute_single_model(
    state: &AppState,
    snapshot: &AppSnapshot,
    request: &ExecuteRequest,
    model: &str,
    last_error: &mut Option<ProxyError>,
) -> Result<Option<ExecuteResult>, ProxyError> {
    let mut targets = snapshot.registry.attempt_targets(model);
    rotate_attempt_targets(&mut targets, state.provider_cursor(model).await);
    let mut retry_budget = 0usize;

    match run_provider_attempts(
        model,
        snapshot.config.retry.max_retries,
        &snapshot.config,
        &targets,
        &mut retry_budget,
        "model",
        |target, target_attempt| {
            let request = request.clone();
            let model = model.to_string();
            let target = target.clone();
            async move {
                let auth_key = auth_cursor_key(&target);
                let auth_start_index = if let Some(key) = auth_key.as_ref() {
                    state.auth_cursor(key).await
                } else {
                    None
                };
                let result = try_target(
                    &state.providers,
                    state,
                    &request,
                    &model,
                    &target,
                    auth_start_index,
                    target_attempt,
                )
                .await?;
                if result.response.is_success() {
                    state
                        .record_provider_success(&model, provider_cursor(&target))
                        .await;
                    if let Some(key) = auth_key {
                        match result.response.auth_index() {
                            Some(auth_index) => {
                                state.record_auth_success(key, auth_index).await;
                            }
                            // api_key success: clear cursor so next request starts at api_key first.
                            None => {
                                state.clear_auth_cursor(&key).await;
                            }
                        }
                    }
                }
                Ok(result)
            }
        },
    )
    .await?
    {
        AttemptLoopResult::Success(result) => Ok(Some(result)),
        AttemptLoopResult::Exhausted {
            last_result,
            last_error: exhausted_error,
        } => {
            if let Some(error) = exhausted_error {
                *last_error = Some(error);
            }
            Ok(last_result)
        }
    }
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::config::{
        BaseProviderConfig, Config, GrokAuth, GrokConfig, OneOrMany, OpenAiChatConfig,
        ProviderConfig,
    };

    use super::*;

    #[tokio::test]
    async fn run_provider_attempts_covers_extra_credentials_before_repeating_single_slot_targets() {
        let targets = vec![
            AttemptTarget {
                provider_type: ProviderType::Grok,
                provider_index: 0,
                config_index: 0,
                config: ProviderConfig::Grok(GrokConfig {
                    base: base_config("https://grok.example/v1", "sk-grok"),
                    auth: OneOrMany::Many(vec![
                        GrokAuth {
                            access_token: Some("auth-a".to_string()),
                            ..empty_grok_auth()
                        },
                        GrokAuth {
                            access_token: Some("auth-b".to_string()),
                            ..empty_grok_auth()
                        },
                    ]),
                }),
            },
            AttemptTarget {
                provider_type: ProviderType::Chat,
                provider_index: 1,
                config_index: 0,
                config: ProviderConfig::OpenAiChat(OpenAiChatConfig {
                    base: base_config("https://chat.example/v1", "sk-chat"),
                }),
            },
        ];
        let mut config = Config::default();
        config.retry.backoff_step_ms = 0;
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let mut retry_budget = 0usize;

        let result =
            run_provider_attempts("model", 1, &config, &targets, &mut retry_budget, "test", {
                let attempts = Arc::clone(&attempts);
                move |target, target_attempt| {
                    let attempts = Arc::clone(&attempts);
                    let provider_type = target.provider_type;
                    async move {
                        attempts
                            .lock()
                            .unwrap()
                            .push((provider_type, target_attempt));
                        Ok(ExecuteResult {
                            provider_type,
                            response: UpstreamResponse::NonStream {
                                status: 500,
                                headers: Default::default(),
                                body: Bytes::new(),
                                auth_index: None,
                            },
                        })
                    }
                }
            })
            .await
            .unwrap();

        assert!(matches!(result, AttemptLoopResult::Exhausted { .. }));
        assert_eq!(
            attempts.lock().unwrap().as_slice(),
            &[
                (ProviderType::Grok, 1),
                (ProviderType::Chat, 1),
                (ProviderType::Grok, 2),
                (ProviderType::Grok, 3),
            ],
        );
    }

    fn base_config(base_url: &str, api_key: &str) -> BaseProviderConfig {
        BaseProviderConfig {
            enabled: true,
            models: vec!["model".to_string()],
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            headers: Default::default(),
        }
    }

    fn empty_grok_auth() -> GrokAuth {
        GrokAuth {
            access_token: None,
            refresh_token: None,
            id_token: None,
            token_type: None,
            expires_in: None,
            expiry_date: None,
            base_url: None,
            token_endpoint: None,
            headers: None,
            disabled: None,
        }
    }
}
