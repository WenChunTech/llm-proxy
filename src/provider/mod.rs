mod claude;
mod codex;
mod gemini;
mod grok;
pub mod oauth;
mod http;
mod openai;
mod request_rewrite;

pub mod executor;
pub mod registry;
pub mod types;

pub use request_rewrite::{has_rewrite, rewrite_request, wire_protocol};

use bytes::Bytes;
use reqwest::header::{HeaderMap as ReqwestHeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use crate::{
    config::ProviderConfig,
    error::ProxyError,
    protocol,
    provider::types::{HeaderMap, ProviderType},
    stream::convert::{StreamContext, StreamConverterImpl},
};

use self::{
    claude::ClaudeProvider,
    codex::CodexProvider,
    gemini::GeminiProvider,
    grok::GrokProvider,
    openai::{OpenAiChatProvider, OpenAiResponsesProvider},
};

#[derive(Clone)]
pub struct Providers {
    chat: OpenAiChatProvider,
    responses: OpenAiResponsesProvider,
    claude: ClaudeProvider,
    gemini: GeminiProvider,
    codex: CodexProvider,
    grok: GrokProvider,
}

impl Default for Providers {
    fn default() -> Self {
        Self::new()
    }
}

impl Providers {
    pub fn new() -> Self {
        Self {
            chat: OpenAiChatProvider,
            responses: OpenAiResponsesProvider,
            claude: ClaudeProvider,
            gemini: GeminiProvider,
            codex: CodexProvider,
            grok: GrokProvider,
        }
    }

    pub fn prepare_request(
        &self,
        provider_type: ProviderType,
        body: Value,
        source: ProviderType,
        is_streaming: bool,
    ) -> Result<Value, ProxyError> {
        // Provider-scoped pipeline: optional convert → optional rewrite → common.
        // Individual providers only handle transport (send_request).
        request_rewrite::prepare_request(provider_type, body, source, is_streaming)
    }

    pub async fn send_request(
        &self,
        request: SendRequest<'_>,
    ) -> Result<UpstreamResponse, ProxyError> {
        let response = match (request.provider_type, request.config) {
            (ProviderType::Chat, ProviderConfig::OpenAiChat(config)) => {
                self.chat.send_request(request.with_config(config)).await
            }
            (ProviderType::Responses, ProviderConfig::OpenAiResponses(config)) => {
                self.responses
                    .send_request(request.with_config(config))
                    .await
            }
            (ProviderType::Claude, ProviderConfig::Claude(config)) => {
                self.claude.send_request(request.with_config(config)).await
            }
            (ProviderType::Gemini, ProviderConfig::Gemini(config)) => {
                self.gemini.send_request(request.with_config(config)).await
            }
            (ProviderType::Codex, ProviderConfig::Codex(config)) => {
                self.codex.send_request(request.with_config(config)).await
            }
            (ProviderType::Grok, ProviderConfig::Grok(config)) => {
                self.grok.send_request(request.with_config(config)).await
            }
            _ => Err(ProxyError::InvalidRequest(
                "provider/config type mismatch".to_string(),
            )),
        };

        match &response {
            Ok(upstream) => {
                log_upstream_response_body(upstream, request.provider_type, request.model)
            }
            Err(error) => {
                let upstream_status_code =
                    error.upstream_status_code().map(|status| status.as_u16());
                let upstream_url = error.upstream_url();
                tracing::warn!(
                    provider = ?request.provider_type,
                    model = %request.model,
                    config_index = request.config_index,
                    base_url = request.config.base_url().unwrap_or("<provider default>"),
                    status_code = error.status_code().as_u16(),
                    upstream_status_code = ?upstream_status_code,
                    upstream_url = upstream_url.as_deref().unwrap_or(""),
                    error = %error,
                    "provider request failed"
                );
            }
        }

        response
    }

    pub fn convert_response(
        &self,
        source: ProviderType,
        body: Value,
        target: ProviderType,
    ) -> Result<Value, ProxyError> {
        protocol::convert_response(body, source.response_protocol(), target)
    }

    pub fn stream_converter(
        &self,
        source: ProviderType,
        target: ProviderType,
        context: StreamContext,
    ) -> StreamConverterImpl {
        StreamConverterImpl::new(source.response_protocol(), target, context)
    }
}

pub struct SendRequest<'a> {
    pub state: Option<&'a crate::state::AppState>,
    pub client: &'a reqwest::Client,
    pub is_streaming: bool,
    pub provider_type: ProviderType,
    pub request: Value,
    pub config: &'a ProviderConfig,
    pub config_index: usize,
    pub forwarded_headers: &'a HeaderMap,
    pub model: &'a str,
    pub auth_start_index: Option<usize>,
    pub target_attempt: usize,
}

impl<'a> SendRequest<'a> {
    fn with_config<C>(&'a self, config: &'a C) -> TypedSendRequest<'a, C> {
        TypedSendRequest {
            state: self.state,
            client: self.client,
            is_streaming: self.is_streaming,
            request: self.request.clone(),
            config,
            config_index: self.config_index,
            forwarded_headers: self.forwarded_headers,
            model: self.model,
            auth_start_index: self.auth_start_index,
            target_attempt: self.target_attempt,
        }
    }
}

struct TypedSendRequest<'a, C> {
    state: Option<&'a crate::state::AppState>,
    client: &'a reqwest::Client,
    is_streaming: bool,
    request: Value,
    config: &'a C,
    config_index: usize,
    forwarded_headers: &'a HeaderMap,
    model: &'a str,
    auth_start_index: Option<usize>,
    target_attempt: usize,
}

#[derive(Debug)]
pub enum UpstreamResponse {
    NonStream {
        status: u16,
        headers: HeaderMap,
        body: Bytes,
        auth_index: Option<usize>,
    },
    Stream {
        status: u16,
        headers: HeaderMap,
        response: reqwest::Response,
        auth_index: Option<usize>,
    },
}

impl UpstreamResponse {
    pub fn status(&self) -> u16 {
        match self {
            Self::NonStream { status, .. } | Self::Stream { status, .. } => *status,
        }
    }

    pub fn headers(&self) -> &HeaderMap {
        match self {
            Self::NonStream { headers, .. } | Self::Stream { headers, .. } => headers,
        }
    }

    pub fn auth_index(&self) -> Option<usize> {
        match self {
            Self::NonStream { auth_index, .. } | Self::Stream { auth_index, .. } => *auth_index,
        }
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status())
    }

    /// Returns the response body text when available (non-stream responses).
    /// Failed upstream requests are always collected as NonStream, so this
    /// is populated for error responses even when the original request was streaming.
    pub fn body_text(&self) -> Option<String> {
        match self {
            Self::NonStream { body, .. } => Some(String::from_utf8_lossy(body).to_string()),
            Self::Stream { .. } => None,
        }
    }
}

fn log_upstream_response_body(
    response: &UpstreamResponse,
    provider_type: ProviderType,
    model: &str,
) {
    if let UpstreamResponse::NonStream { status, body, .. } = response {
        let raw_response_body = String::from_utf8_lossy(body);
        if (200..300).contains(status) {
            tracing::debug!(
                provider = ?provider_type,
                model = %model,
                status_code = status,
                raw_response_body = %raw_response_body,
                "upstream raw response body"
            );
        } else {
            tracing::warn!(
                provider = ?provider_type,
                model = %model,
                status_code = status,
                raw_response_body = %raw_response_body,
                "upstream request returned non-success response body"
            );
        }
    }
}

async fn collect_response(
    resp: reqwest::Response,
    is_streaming: bool,
) -> Result<UpstreamResponse, ProxyError> {
    collect_response_with_auth(resp, is_streaming, None).await
}

async fn collect_response_with_auth(
    resp: reqwest::Response,
    is_streaming: bool,
    auth_index: Option<usize>,
) -> Result<UpstreamResponse, ProxyError> {
    let status = resp.status().as_u16();
    let headers = response_headers(resp.headers());
    if is_streaming && (200..300).contains(&status) {
        Ok(UpstreamResponse::Stream {
            status,
            headers,
            response: resp,
            auth_index,
        })
    } else {
        let body = resp.bytes().await?;
        Ok(UpstreamResponse::NonStream {
            status,
            headers,
            body,
            auth_index,
        })
    }
}

fn response_headers(headers: &ReqwestHeaderMap) -> HeaderMap {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect()
}

fn reqwest_headers(headers: &HeaderMap) -> Result<ReqwestHeaderMap, ProxyError> {
    let mut out = ReqwestHeaderMap::new();
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|err| {
            ProxyError::InvalidRequest(format!("invalid header name {name}: {err}"))
        })?;
        let value = HeaderValue::from_str(value).map_err(|err| {
            ProxyError::InvalidRequest(format!("invalid header value for {name}: {err}"))
        })?;
        out.insert(name, value);
    }
    Ok(out)
}
