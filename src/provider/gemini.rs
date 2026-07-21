use serde_json::Value;

use crate::{
    config::GeminiConfig, error::ProxyError, middleware::headers::{apply_map_headers, merge_headers}, protocol,
    provider::types::ProviderType,
};

use super::{TypedSendRequest, UpstreamResponse, collect_response, reqwest_headers};

#[derive(Clone)]
pub(super) struct GeminiProvider;

impl GeminiProvider {
    pub(super) fn prepare_request(
        &self,
        body: Value,
        source: ProviderType,
        _is_streaming: bool,
    ) -> Result<Value, ProxyError> {
        protocol::convert_request(body, source, ProviderType::Gemini)
    }

    pub(super) async fn send_request(
        &self,
        request: TypedSendRequest<'_, GeminiConfig>,
    ) -> Result<UpstreamResponse, ProxyError> {
        let action = if request.is_streaming {
            "streamGenerateContent?alt=sse"
        } else {
            "generateContent"
        };
        let url = format!(
            "{}/v1beta/models/{}:{}",
            request.config.base.base_url.trim_end_matches('/'),
            request.model,
            action
        );
        let mut headers = merge_headers(
            request.forwarded_headers,
            &[
                ("content-type", "application/json".to_string()),
                ("x-goog-api-key", request.config.base.api_key.clone()),
                (
                    "authorization",
                    format!("Bearer {}", request.config.base.api_key),
                ),
            ],
        );
        apply_map_headers(&mut headers, &request.config.base.headers);
        let resp = request
            .client
            .post(url)
            .headers(reqwest_headers(&headers)?)
            .json(&request.request)
            .send()
            .await?;
        collect_response(resp, request.is_streaming).await
    }
}
