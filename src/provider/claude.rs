use crate::{config::ClaudeConfig, error::ProxyError};

use super::{
    TypedSendRequest, UpstreamResponse, collect_response,
    http::{api_key_headers, post_json},
};

#[derive(Clone)]
pub(super) struct ClaudeProvider;

impl ClaudeProvider {
    pub(super) async fn send_request(
        &self,
        request: TypedSendRequest<'_, ClaudeConfig>,
    ) -> Result<UpstreamResponse, ProxyError> {
        let headers = api_key_headers(
            request.forwarded_headers,
            &[
                ("content-type", "application/json".to_string()),
                ("x-api-key", request.config.base.api_key.clone()),
                ("anthropic-version", "2023-06-01".to_string()),
            ],
            &request.config.base.headers,
        );
        let resp = post_json(
            request.client,
            format!(
                "{}/v1/messages",
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
