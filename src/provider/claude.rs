use serde_json::Value;

use crate::{
    config::ClaudeConfig, error::ProxyError, middleware::headers::{apply_map_headers, merge_headers}, protocol,
    provider::types::ProviderType,
};

use super::{TypedSendRequest, UpstreamResponse, collect_response, reqwest_headers};

#[derive(Clone)]
pub(super) struct ClaudeProvider;

impl ClaudeProvider {
    pub(super) fn prepare_request(
        &self,
        body: Value,
        source: ProviderType,
        is_streaming: bool,
    ) -> Result<Value, ProxyError> {
        let mut body = protocol::convert_request(body, source, ProviderType::Claude)?;
        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".to_string(), Value::Bool(is_streaming));
        }
        Ok(body)
    }

    pub(super) async fn send_request(
        &self,
        request: TypedSendRequest<'_, ClaudeConfig>,
    ) -> Result<UpstreamResponse, ProxyError> {
        let mut headers = merge_headers(
            request.forwarded_headers,
            &[
                ("content-type", "application/json".to_string()),
                ("x-api-key", request.config.base.api_key.clone()),
                ("anthropic-version", "2023-06-01".to_string()),
            ],
        );
        apply_map_headers(&mut headers, &request.config.base.headers);
        let resp = request
            .client
            .post(format!(
                "{}/v1/messages",
                request.config.base.base_url.trim_end_matches('/')
            ))
            .headers(reqwest_headers(&headers)?)
            .json(&request.request)
            .send()
            .await?;
        collect_response(resp, request.is_streaming).await
    }
}
