use serde_json::Value;

use crate::{
    config::GrokConfig,
    error::ProxyError,
    middleware::headers::{apply_map_headers, apply_optional_map_headers, merge_headers},
    provider::types::ProviderType,
    state::AuthCursorKey,
};

use super::{
    TypedSendRequest, UpstreamResponse, collect_response_with_auth,
    oauth::{
        DEFAULT_GROK_BASE_URL, SelectedCredential, grok_access_token, grok_oauth_base_url,
        responses_endpoint, select_oauth_credential,
    },
    reqwest_headers,
};

#[derive(Clone)]
pub(super) struct GrokProvider;

impl GrokProvider {
    pub(super) async fn send_request(
        &self,
        request: TypedSendRequest<'_, GrokConfig>,
    ) -> Result<UpstreamResponse, ProxyError> {
        let provider_base_url = request.config.base.base_url.trim();
        let selected = select_oauth_credential(
            &request.config.base.api_key,
            &request.config.auth,
            request.auth_start_index,
            request.target_attempt,
            "Grok",
        )?;
        let (token, extra_headers, base_url, auth_index) = match selected {
            SelectedCredential::ApiKey { token } => (
                token,
                None,
                if provider_base_url.is_empty() {
                    DEFAULT_GROK_BASE_URL.to_string()
                } else {
                    provider_base_url.trim_end_matches('/').to_string()
                },
                None,
            ),
            SelectedCredential::Auth {
                index: auth_index,
                mut auth,
            } => {
                let auth_key = AuthCursorKey {
                    provider_type: ProviderType::Grok,
                    base_url: provider_base_url.trim_end_matches('/').to_string(),
                    config_index: request.config_index,
                };
                if let Some(state) = request.state
                    && let Some(cached_auth) = state.cached_grok_auth(&auth_key, auth_index).await
                {
                    auth = cached_auth;
                }
                let access_token = grok_access_token(request.client, &mut auth).await?;
                if access_token.refreshed
                    && let Some(state) = request.state
                {
                    state
                        .record_grok_auth(&auth_key, auth_index, auth.clone())
                        .await;
                }
                let base_url = grok_oauth_base_url(
                    (!provider_base_url.is_empty()).then_some(provider_base_url),
                    &auth,
                );
                (
                    access_token.token,
                    auth.headers.clone(),
                    base_url,
                    Some(auth_index),
                )
            }
        };
        tracing::info!(
            provider = ?ProviderType::Grok,
            model = %request.model,
            base_url = %base_url,
            auth_index,
            "provider resolved base_url"
        );
        let mut body = request.request;
        let obj = body.as_object_mut().ok_or_else(|| {
            ProxyError::InvalidRequest("Grok request body must be a JSON object".to_string())
        })?;
        obj.insert(
            "model".to_string(),
            Value::String(request.model.to_string()),
        );
        obj.insert("stream".to_string(), Value::Bool(request.is_streaming));

        let mut headers = merge_headers(
            request.forwarded_headers,
            &[
                ("content-type", "application/json".to_string()),
                ("authorization", format!("Bearer {token}")),
                (
                    "accept",
                    if request.is_streaming {
                        "text/event-stream"
                    } else {
                        "application/json"
                    }
                    .to_string(),
                ),
            ],
        );
        // Priority: custom provider headers override auth headers on conflicts.
        apply_optional_map_headers(&mut headers, extra_headers.as_ref());
        apply_map_headers(&mut headers, &request.config.base.headers);

        let resp = request
            .client
            .post(responses_endpoint(&base_url)?)
            .headers(reqwest_headers(&headers)?)
            .json(&body)
            .send()
            .await?;
        collect_response_with_auth(resp, request.is_streaming, auth_index).await
    }
}
