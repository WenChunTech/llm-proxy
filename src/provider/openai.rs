use serde_json::Value;

use crate::{
    config::{OpenAiChatConfig, OpenAiResponsesConfig},
    error::ProxyError,
    protocol,
    provider::types::ProviderType,
};

use super::{
    TypedSendRequest, UpstreamResponse, collect_response,
    http::{bearer_json_headers, post_json},
};

#[derive(Clone)]
pub(super) struct OpenAiChatProvider;

impl OpenAiChatProvider {
    pub(super) fn prepare_request(
        &self,
        body: Value,
        source: ProviderType,
        is_streaming: bool,
    ) -> Result<Value, ProxyError> {
        let mut body = protocol::convert_request(body, source, ProviderType::Chat)?;
        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".to_string(), Value::Bool(is_streaming));
        }
        Ok(body)
    }

    pub(super) async fn send_request(
        &self,
        request: TypedSendRequest<'_, OpenAiChatConfig>,
    ) -> Result<UpstreamResponse, ProxyError> {
        let headers = bearer_json_headers(
            request.forwarded_headers,
            &request.config.base.api_key,
            &request.config.base.headers,
        );
        let resp = post_json(
            request.client,
            format!(
                "{}/chat/completions",
                request.config.base.base_url.trim_end_matches('/')
            ),
            &headers,
            &request.request,
        )?
        .send()
        .await?;
        collect_response(resp, request.is_streaming).await
    }
}

#[derive(Clone)]
pub(super) struct OpenAiResponsesProvider;

impl OpenAiResponsesProvider {
    pub(super) fn prepare_request(
        &self,
        body: Value,
        source: ProviderType,
        is_streaming: bool,
    ) -> Result<Value, ProxyError> {
        let mut body = protocol::convert_request(body, source, ProviderType::Responses)?;
        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".to_string(), Value::Bool(is_streaming));
        }
        Ok(body)
    }

    pub(super) async fn send_request(
        &self,
        request: TypedSendRequest<'_, OpenAiResponsesConfig>,
    ) -> Result<UpstreamResponse, ProxyError> {
        let headers = bearer_json_headers(
            request.forwarded_headers,
            &request.config.base.api_key,
            &request.config.base.headers,
        );
        let resp = post_json(
            request.client,
            format!(
                "{}/responses",
                request.config.base.base_url.trim_end_matches('/')
            ),
            &headers,
            &request.request,
        )?
        .send()
        .await?;
        collect_response(resp, request.is_streaming).await
    }
}
