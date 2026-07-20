use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Chat,
    Responses,
    Claude,
    Gemini,
    Codex,
    Grok,
}

const DEFAULT_PRIORITY: &[ProviderType] = &[
    ProviderType::Chat,
    ProviderType::Responses,
    ProviderType::Claude,
    ProviderType::Gemini,
    ProviderType::Codex,
    ProviderType::Grok,
];

impl ProviderType {
    pub fn default_priority() -> &'static [Self] {
        DEFAULT_PRIORITY
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "openai_chat",
            Self::Responses => "openai_responses",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Codex => "codex",
            Self::Grok => "grok",
        }
    }

    pub fn from_config_id(value: &str) -> Option<Self> {
        match value {
            "openai_chat" => Some(Self::Chat),
            "openai_responses" => Some(Self::Responses),
            "claude" => Some(Self::Claude),
            "gemini" => Some(Self::Gemini),
            "codex" => Some(Self::Codex),
            "grok" | "xai" => Some(Self::Grok),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Chat => "OpenAI Chat",
            Self::Responses => "OpenAI Responses",
            Self::Claude => "Claude",
            Self::Gemini => "Gemini",
            Self::Codex => "Codex",
            Self::Grok => "Grok",
        }
    }

    pub fn uses_openai_models_endpoint(self) -> bool {
        matches!(
            self,
            Self::Chat | Self::Responses | Self::Codex | Self::Grok
        )
    }

    pub fn response_protocol(self) -> Self {
        match self {
            Self::Codex | Self::Grok => Self::Responses,
            other => other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AttemptTarget {
    pub provider_type: ProviderType,
    pub provider_index: usize,
    pub config_index: usize,
    pub config: crate::config::ProviderConfig,
}

impl AttemptTarget {
    pub fn base_url(&self) -> String {
        self.config
            .base_url()
            .map(|base_url| base_url.trim_end_matches('/').to_string())
            .unwrap_or_default()
    }
}

pub type HeaderMap = std::collections::HashMap<String, String>;
