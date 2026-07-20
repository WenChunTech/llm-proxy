use serde_json::Value;

use crate::{
    config::CodexConfig, error::ProxyError, middleware::headers::merge_headers, protocol,
    provider::types::ProviderType, state::AuthCursorKey,
};

use super::{
    TypedSendRequest, UpstreamResponse, collect_response_with_auth,
    oauth::{
        CODEX_USER_AGENT, DEFAULT_CODEX_BASE_URL, codex_access_token, codex_oauth_base_url,
        responses_endpoint, select_codex_auth,
    },
    reqwest_headers,
};

#[derive(Clone)]
pub(super) struct CodexProvider;

impl CodexProvider {
    pub(super) fn prepare_request(
        &self,
        body: Value,
        source: ProviderType,
        is_streaming: bool,
    ) -> Result<Value, ProxyError> {
        let mut body = protocol::convert_request(body, source, ProviderType::Responses)?;
        let obj = body.as_object_mut().ok_or_else(|| {
            ProxyError::InvalidRequest("Codex request body must be a JSON object".to_string())
        })?;
        obj.remove("max_output_tokens");
        obj.remove("temperature");
        obj.insert("store".to_string(), Value::Bool(false));
        obj.insert("stream".to_string(), Value::Bool(is_streaming));
        Ok(body)
    }

    pub(super) async fn send_request(
        &self,
        request: TypedSendRequest<'_, CodexConfig>,
    ) -> Result<UpstreamResponse, ProxyError> {
        let api_key = request.config.base.api_key.trim();
        let api_key = (!api_key.is_empty()).then_some(api_key);
        let provider_base_url = request.config.base.base_url.trim();
        let (token, account_id, base_url, auth_index) = if let Some(api_key) = api_key {
            (
                api_key.to_string(),
                None,
                if provider_base_url.is_empty() {
                    DEFAULT_CODEX_BASE_URL.to_string()
                } else {
                    provider_base_url.trim_end_matches('/').to_string()
                },
                None,
            )
        } else {
            let (auth_index, mut auth) = select_codex_auth(
                &request.config.auth,
                request.auth_start_index,
                request.target_attempt,
            )?;
            let auth_key = AuthCursorKey {
                provider_type: ProviderType::Codex,
                base_url: provider_base_url.trim_end_matches('/').to_string(),
                config_index: request.config_index,
            };
            if let Some(state) = request.state
                && let Some(cached_auth) = state.cached_codex_auth(&auth_key, auth_index).await
            {
                auth = cached_auth;
            }
            let access_token = codex_access_token(request.client, &mut auth).await?;
            if access_token.refreshed
                && let Some(state) = request.state
            {
                state
                    .record_codex_auth(&auth_key, auth_index, auth.clone())
                    .await;
            }
            let base_url = codex_oauth_base_url(
                (!provider_base_url.is_empty()).then_some(provider_base_url),
                &auth,
            );
            (
                access_token.token,
                auth.account_id.clone(),
                base_url,
                Some(auth_index),
            )
        };
        tracing::info!(
            provider = ?ProviderType::Codex,
            model = %request.model,
            base_url = %base_url,
            auth_index,
            "provider resolved base_url"
        );
        let mut body = request.request;
        let obj = body.as_object_mut().ok_or_else(|| {
            ProxyError::InvalidRequest("Codex request body must be a JSON object".to_string())
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
                ("user-agent", CODEX_USER_AGENT.to_string()),
                ("originator", "codex-tui".to_string()),
                ("connection", "Keep-Alive".to_string()),
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
        if let Some(account_id) = account_id.as_ref().filter(|value| !value.is_empty()) {
            headers.insert("chatgpt-account-id".to_string(), account_id.clone());
        }

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
