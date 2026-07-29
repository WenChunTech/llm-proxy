use converter::{
    convert::{
        ChatStreamWrapper, ClaudeStreamWrapper, GeminiCliStreamWrapper, GrokStreamWrapper,
        ResponsesStreamWrapper, StreamState,
    },
    models::{claude, gemini, gemini_cli, grok, openai},
};
use serde_json::Value;

use crate::{
    error::ProxyError,
    provider::{rewrite_response, types::ProviderType},
    stream::sse::{OutboundSseEvent, SseEvent},
};

pub enum StreamConverterImpl {
    Passthrough,
    Protocol(Box<ProtocolStreamConverter>),
}

impl StreamConverterImpl {
    pub fn new(source: ProviderType, target: ProviderType, context: StreamContext) -> Self {
        if source == target {
            return Self::Passthrough;
        }

        let state = initial_state(source, target, context);
        Self::Protocol(Box::new(ProtocolStreamConverter {
            source,
            target,
            state,
        }))
    }

    pub fn convert_event(&mut self, event: SseEvent) -> Result<Vec<OutboundSseEvent>, ProxyError> {
        match self {
            Self::Passthrough => Ok(vec![OutboundSseEvent {
                event: event.event,
                data: event.data,
            }]),
            Self::Protocol(inner) => inner.convert_event(event),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StreamContext {
    pub responses_request: Option<openai::responses::Request>,
    pub grok_request: Option<grok::Request>,
}

impl StreamContext {
    pub fn from_request(target: ProviderType, body: &Value) -> Self {
        match target {
            // Deserialize from borrowed Value to avoid cloning large request bodies.
            ProviderType::Responses => Self {
                responses_request: serde::Deserialize::deserialize(body).ok(),
                grok_request: None,
            },
            ProviderType::Grok => Self {
                responses_request: None,
                grok_request: serde::Deserialize::deserialize(body).ok(),
            },
            _ => Self::default(),
        }
    }
}

pub struct ProtocolStreamConverter {
    source: ProviderType,
    target: ProviderType,
    state: StreamState,
}

impl ProtocolStreamConverter {
    fn convert_event(&mut self, event: SseEvent) -> Result<Vec<OutboundSseEvent>, ProxyError> {
        if event.data.trim() == "[DONE]" || event.data.trim().is_empty() {
            return Ok(Vec::new());
        }

        let data: Value = serde_json::from_str(&event.data)?;
        let data = rewrite_response(self.source, data)?;
        let chunks = match self.source {
            ProviderType::Chat => self.convert_chat(data)?,
            ProviderType::Responses => self.convert_responses(data)?,
            ProviderType::Claude => self.convert_claude(data)?,
            ProviderType::Gemini => self.convert_gemini(data)?,
            ProviderType::Grok => self.convert_grok(data)?,
            ProviderType::Codex => Vec::new(),
        };
        Ok(chunks)
    }

    fn convert_chat(&mut self, data: Value) -> Result<Vec<OutboundSseEvent>, ProxyError> {
        let chunk: openai::chat::Response = serde_json::from_value(data)?;
        match self.target {
            ProviderType::Responses => {
                let wrapper = ChatStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ResponsesStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Claude => {
                let wrapper = ChatStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ClaudeStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Gemini => {
                let wrapper = ChatStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::GeminiCliStreamsWrapper = wrapper.into();
                self.state = converted.state;
                let chunks: Vec<gemini::Response> =
                    converted.chunks.into_iter().map(Into::into).collect();
                values_to_events(chunks)
            }
            ProviderType::Grok => {
                let wrapper = ChatStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::GrokStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Chat => Ok(Vec::new()),
            ProviderType::Codex => Ok(Vec::new()),
        }
    }

    fn convert_responses(&mut self, data: Value) -> Result<Vec<OutboundSseEvent>, ProxyError> {
        let chunk: openai::responses::StreamResponse = serde_json::from_value(data)?;
        match self.target {
            ProviderType::Chat => {
                let wrapper = ResponsesStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ChatStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Claude => {
                let wrapper = ResponsesStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ClaudeStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Gemini => {
                let wrapper = ResponsesStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::GeminiCliStreamsWrapper = wrapper.into();
                self.state = converted.state;
                let chunks: Vec<gemini::Response> =
                    converted.chunks.into_iter().map(Into::into).collect();
                values_to_events(chunks)
            }
            ProviderType::Responses => Ok(Vec::new()),
            ProviderType::Codex | ProviderType::Grok => Ok(Vec::new()),
        }
    }

    fn convert_claude(&mut self, data: Value) -> Result<Vec<OutboundSseEvent>, ProxyError> {
        let chunk: claude::StreamResponse = serde_json::from_value(data)?;
        match self.target {
            ProviderType::Chat => {
                let wrapper = ClaudeStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ChatStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Responses => {
                let wrapper = ClaudeStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ResponsesStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Gemini => {
                let wrapper = ClaudeStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::GeminiCliStreamsWrapper = wrapper.into();
                self.state = converted.state;
                let chunks: Vec<gemini::Response> =
                    converted.chunks.into_iter().map(Into::into).collect();
                values_to_events(chunks)
            }
            ProviderType::Claude => Ok(Vec::new()),
            ProviderType::Codex | ProviderType::Grok => Ok(Vec::new()),
        }
    }

    fn convert_gemini(&mut self, data: Value) -> Result<Vec<OutboundSseEvent>, ProxyError> {
        let chunk: gemini::Response = serde_json::from_value(data)?;
        let chunk: gemini_cli::Response = chunk.into();
        match self.target {
            ProviderType::Chat => {
                let wrapper = GeminiCliStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ChatStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Responses => {
                let wrapper = GeminiCliStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ResponsesStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Claude => {
                let wrapper = GeminiCliStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ClaudeStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Gemini => Ok(Vec::new()),
            ProviderType::Codex | ProviderType::Grok => Ok(Vec::new()),
        }
    }

    fn convert_grok(&mut self, data: Value) -> Result<Vec<OutboundSseEvent>, ProxyError> {
        let chunk: grok::StreamResponse = serde_json::from_value(data)?;
        match self.target {
            ProviderType::Chat => {
                let wrapper = GrokStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ChatStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Claude => {
                let wrapper = GrokStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::ClaudeStreamsWrapper = wrapper.into();
                self.state = converted.state;
                values_to_events(converted.chunks)
            }
            ProviderType::Gemini => {
                let wrapper = GrokStreamWrapper {
                    chunk,
                    state: self.take_state(),
                };
                let converted: converter::convert::GeminiCliStreamsWrapper = wrapper.into();
                self.state = converted.state;
                let chunks: Vec<gemini::Response> =
                    converted.chunks.into_iter().map(Into::into).collect();
                values_to_events(chunks)
            }
            ProviderType::Responses => {
                // Grok stream events map 1:1 onto OpenAI Responses events.
                let converted: openai::responses::StreamResponse = chunk.into();
                values_to_events(vec![converted])
            }
            ProviderType::Grok => Ok(Vec::new()),
            ProviderType::Codex => Ok(Vec::new()),
        }
    }

    fn take_state(&mut self) -> StreamState {
        std::mem::replace(&mut self.state, StreamState::Empty)
    }
}

fn initial_state(
    source: ProviderType,
    target: ProviderType,
    context: StreamContext,
) -> StreamState {
    match (source, target) {
        (ProviderType::Chat, ProviderType::Responses) => {
            let state = StreamState::chat_to_responses();
            if let Some(request) = context.responses_request {
                state.with_chat_request(request)
            } else {
                state
            }
        }
        (ProviderType::Chat, ProviderType::Claude) => StreamState::chat_to_claude(),
        (ProviderType::Chat, ProviderType::Gemini) => StreamState::chat_to_gemini_cli(),
        (ProviderType::Responses, ProviderType::Chat) => StreamState::responses_to_chat(),
        (ProviderType::Responses, ProviderType::Claude) => StreamState::responses_to_claude(),
        (ProviderType::Responses, ProviderType::Gemini) => StreamState::responses_to_gemini_cli(),
        (ProviderType::Claude, ProviderType::Chat) => StreamState::claude_to_chat(),
        (ProviderType::Claude, ProviderType::Responses) => {
            let state = StreamState::claude_to_responses();
            if let Some(request) = context.responses_request {
                state.with_claude_request(request)
            } else {
                state
            }
        }
        (ProviderType::Claude, ProviderType::Gemini) => StreamState::claude_to_gemini_cli(),
        (ProviderType::Gemini, ProviderType::Chat) => StreamState::gemini_cli_to_chat(),
        (ProviderType::Gemini, ProviderType::Responses) => {
            let state = StreamState::gemini_cli_to_responses();
            if let Some(request) = context.responses_request {
                state.with_gemini_cli_request(request)
            } else {
                state
            }
        }
        (ProviderType::Gemini, ProviderType::Claude) => StreamState::gemini_cli_to_claude(),
        // Grok as upstream source: native Grok stream events -> client protocol.
        (ProviderType::Grok, ProviderType::Chat) => StreamState::grok_to_chat(),
        (ProviderType::Grok, ProviderType::Claude) => StreamState::grok_to_claude(),
        (ProviderType::Grok, ProviderType::Gemini) => StreamState::grok_to_gemini_cli(),
        (ProviderType::Grok, ProviderType::Responses) => StreamState::grok_to_responses(),
        // Client protocol -> Grok upstream: seed the source-derived request when available.
        (ProviderType::Chat, ProviderType::Grok) => {
            let state = StreamState::chat_to_grok();
            if let Some(request) = context.grok_request {
                state.with_chat_grok_request(request)
            } else {
                state
            }
        }
        (ProviderType::Claude, ProviderType::Grok) => {
            let state = StreamState::claude_to_grok();
            if let Some(request) = context.grok_request {
                state.with_claude_grok_request(request)
            } else {
                state
            }
        }
        (ProviderType::Gemini, ProviderType::Grok) => {
            let state = StreamState::gemini_cli_to_grok();
            if let Some(request) = context.grok_request {
                state.with_gemini_cli_grok_request(request)
            } else {
                state
            }
        }
        (ProviderType::Responses, ProviderType::Grok) => StreamState::responses_to_grok(),
        _ => StreamState::Empty,
    }
}

fn values_to_events<T: serde::Serialize>(
    chunks: Vec<T>,
) -> Result<Vec<OutboundSseEvent>, ProxyError> {
    chunks
        .into_iter()
        .map(|chunk| {
            let value = serde_json::to_value(chunk)?;
            let event = value
                .get("type")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            Ok(OutboundSseEvent {
                event,
                data: serde_json::to_string(&value)?,
            })
        })
        .collect()
}
