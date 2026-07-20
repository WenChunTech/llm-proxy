use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing_subscriber::EnvFilter;

use crate::{error::ProxyError, provider::types::ProviderType};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub port: u16,
    pub api_key: Option<String>,
    pub log_level: Option<String>,
    pub model_priority: Vec<String>,
    pub fallback_models: Vec<String>,
    pub providers: ProviderGroups,
    pub retry: RetryConfig,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 3000,
            api_key: None,
            log_level: None,
            model_priority: Vec::new(),
            fallback_models: Vec::new(),
            providers: ProviderGroups::default(),
            retry: RetryConfig::default(),
            extra: HashMap::new(),
        }
    }
}

impl Config {
    pub fn bind_addr(&self) -> String {
        format!("0.0.0.0:{}", self.port)
    }
}

fn parse_config_json(raw: &str) -> Result<Config, ProxyError> {
    let value: Value = serde_json::from_str(raw)?;
    parse_config_value(value)
}

fn parse_config_value(value: Value) -> Result<Config, ProxyError> {
    let explicit_port = value.get("port").is_some();
    let mut config: Config = serde_json::from_value(value.clone())?;
    // Drop legacy `server` blob from flatten extras so it is not rewritten on save.
    config.extra.remove("server");
    if !explicit_port {
        if let Some(bind) = value
            .get("server")
            .and_then(|server| server.get("bind"))
            .and_then(Value::as_str)
        {
            if let Some(port) = parse_bind_port(bind) {
                config.port = port;
            }
        }
    }
    Ok(config)
}

fn parse_bind_port(bind: &str) -> Option<u16> {
    bind.rsplit_once(':')
        .and_then(|(_, port)| port.trim().parse().ok())
        .filter(|port| *port > 0)
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub backoff_step_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            backoff_step_ms: 5_000,
        }
    }
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ProviderGroups {
    pub openai_chat: Vec<OpenAiChatConfig>,
    pub openai_responses: Vec<OpenAiResponsesConfig>,
    pub claude: Vec<ClaudeConfig>,
    pub gemini: Vec<GeminiConfig>,
    pub codex: Vec<CodexConfig>,
    pub grok: Vec<GrokConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BaseProviderConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenAiChatConfig {
    #[serde(flatten)]
    pub base: BaseProviderConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenAiResponsesConfig {
    #[serde(flatten)]
    pub base: BaseProviderConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeConfig {
    #[serde(flatten)]
    pub base: BaseProviderConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeminiConfig {
    #[serde(flatten)]
    pub base: BaseProviderConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> Default for OneOrMany<T> {
    fn default() -> Self {
        Self::Many(Vec::new())
    }
}

impl<T> OneOrMany<T> {
    pub fn enabled_items_with_indices(&self) -> Vec<(usize, &T)>
    where
        T: AuthEnabled,
    {
        match self {
            Self::One(item) => {
                if item.disabled() {
                    Vec::new()
                } else {
                    vec![(0, item)]
                }
            }
            Self::Many(items) => items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| (!item.disabled()).then_some((index, item)))
                .collect(),
        }
    }
}

pub trait AuthEnabled {
    fn disabled(&self) -> bool;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodexConfig {
    #[serde(flatten)]
    pub base: BaseProviderConfig,
    #[serde(default)]
    pub auth: OneOrMany<CodexAuth>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodexAuth {
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan_type: Option<String>,
    #[serde(default)]
    pub expiry_date: Option<i64>,
    #[serde(default)]
    pub disabled: Option<bool>,
}

impl AuthEnabled for CodexAuth {
    fn disabled(&self) -> bool {
        self.disabled.unwrap_or(false)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrokConfig {
    #[serde(flatten)]
    pub base: BaseProviderConfig,
    #[serde(default)]
    pub auth: OneOrMany<GrokAuth>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrokAuth {
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub expiry_date: Option<i64>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub token_endpoint: Option<String>,
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub disabled: Option<bool>,
}

impl AuthEnabled for GrokAuth {
    fn disabled(&self) -> bool {
        self.disabled.unwrap_or(false)
    }
}

#[derive(Debug, Clone)]
pub enum ProviderConfig {
    OpenAiChat(OpenAiChatConfig),
    OpenAiResponses(OpenAiResponsesConfig),
    Claude(ClaudeConfig),
    Gemini(GeminiConfig),
    Codex(CodexConfig),
    Grok(GrokConfig),
}

impl ProviderConfig {
    pub fn base(&self) -> &BaseProviderConfig {
        match self {
            Self::OpenAiChat(c) => &c.base,
            Self::OpenAiResponses(c) => &c.base,
            Self::Claude(c) => &c.base,
            Self::Gemini(c) => &c.base,
            Self::Codex(c) => &c.base,
            Self::Grok(c) => &c.base,
        }
    }

    pub fn models(&self) -> &[String] {
        &self.base().models
    }

    pub fn enabled(&self) -> bool {
        self.base().enabled
    }

    pub fn base_url(&self) -> Option<&str> {
        let base_url = self.base().base_url.trim();
        (!base_url.is_empty()).then_some(base_url)
    }
}

impl ProviderGroups {
    pub fn configs_for(&self, provider_type: ProviderType) -> Vec<ProviderConfig> {
        match provider_type {
            ProviderType::Chat => self
                .openai_chat
                .iter()
                .cloned()
                .map(ProviderConfig::OpenAiChat)
                .collect(),
            ProviderType::Responses => self
                .openai_responses
                .iter()
                .cloned()
                .map(ProviderConfig::OpenAiResponses)
                .collect(),
            ProviderType::Claude => self
                .claude
                .iter()
                .cloned()
                .map(ProviderConfig::Claude)
                .collect(),
            ProviderType::Gemini => self
                .gemini
                .iter()
                .cloned()
                .map(ProviderConfig::Gemini)
                .collect(),
            ProviderType::Codex => self
                .codex
                .iter()
                .cloned()
                .map(ProviderConfig::Codex)
                .collect(),
            ProviderType::Grok => self
                .grok
                .iter()
                .cloned()
                .map(ProviderConfig::Grok)
                .collect(),
        }
    }

    pub fn iter_configs(&self) -> Vec<(ProviderType, usize, ProviderConfig)> {
        let mut configs = Vec::new();
        for provider_type in ProviderType::default_priority() {
            for (index, config) in self.configs_for(*provider_type).into_iter().enumerate() {
                configs.push((*provider_type, index, config));
            }
        }
        configs
    }

    pub fn from_configs(configs: impl IntoIterator<Item = ProviderConfig>) -> Self {
        let mut groups = Self::default();
        for config in configs {
            groups.push_config(config);
        }
        groups
    }

    pub fn push_config(&mut self, config: ProviderConfig) {
        match config {
            ProviderConfig::OpenAiChat(config) => self.openai_chat.push(config),
            ProviderConfig::OpenAiResponses(config) => self.openai_responses.push(config),
            ProviderConfig::Claude(config) => self.claude.push(config),
            ProviderConfig::Gemini(config) => self.gemini.push(config),
            ProviderConfig::Codex(config) => self.codex.push(config),
            ProviderConfig::Grok(config) => self.grok.push(config),
        }
    }
}

pub fn load_config() -> Result<LoadedConfig, ProxyError> {
    let (config, source_path) = if let Some(path) = cli_config_path() {
        let config = read_config_file(path.clone())?;
        (config, Some(path))
    } else if let Ok(raw) =
        std::env::var("APP_CONFIG_JSON").or_else(|_| std::env::var("APP_CONFIG"))
    {
        (parse_config_json(&raw)?, None)
    } else if let Ok(raw) = fs::read_to_string("config.json") {
        (parse_config_json(&raw)?, Some(PathBuf::from("config.json")))
    } else {
        (Config::default(), Some(PathBuf::from("config.json")))
    };

    validate_config(&config)?;
    Ok(LoadedConfig {
        config,
        source_path,
    })
}

pub fn validate_config(config: &Config) -> Result<(), ProxyError> {
    let mut seen = HashSet::new();
    for model in &config.fallback_models {
        if !seen.insert(model) {
            return Err(ProxyError::Config(format!(
                "duplicate fallback model: {model}"
            )));
        }
    }

    if let Some(log_level) = config
        .log_level
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        EnvFilter::try_new(log_level).map_err(|error| {
            ProxyError::Config(format!("invalid log_level '{log_level}': {error}"))
        })?;
    }
    Ok(())
}

fn cli_config_path() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            return args.next().map(PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix("--config=") {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn read_config_file(path: PathBuf) -> Result<Config, ProxyError> {
    let raw = fs::read_to_string(path)?;
    parse_config_json(&raw)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn port_config_is_read_directly() {
        let config = parse_config_value(json!({
            "port": 7001
        }))
        .unwrap();

        assert_eq!(config.port, 7001);
        assert_eq!(config.bind_addr(), "0.0.0.0:7001");
        assert!(!config.extra.contains_key("server"));
    }

    #[test]
    fn legacy_server_bind_migrates_to_port() {
        let config = parse_config_value(json!({
            "server": { "bind": "0.0.0.0:7001" }
        }))
        .unwrap();

        assert_eq!(config.port, 7001);
        assert!(!config.extra.contains_key("server"));
    }

    #[test]
    fn explicit_port_wins_over_legacy_server_bind() {
        let config = parse_config_value(json!({
            "port": 4000,
            "server": { "bind": "0.0.0.0:7001" }
        }))
        .unwrap();

        assert_eq!(config.port, 4000);
        assert!(!config.extra.contains_key("server"));
    }

    #[test]
    fn log_level_config_is_read_directly() {
        let config = parse_config_value(json!({
            "log_level": "debug"
        }))
        .unwrap();

        assert_eq!(config.log_level.as_deref(), Some("debug"));
        validate_config(&config).unwrap();
    }

    #[test]
    fn invalid_log_level_is_rejected() {
        let config = Config {
            log_level: Some("llm_proxy=verbose".to_string()),
            ..Default::default()
        };

        let error = validate_config(&config).unwrap_err();
        assert!(error.to_string().contains("invalid log_level"));
    }

    #[test]
    fn codex_auth_allows_access_token_without_refresh_token() {
        let auth: CodexAuth = serde_json::from_value(json!({
            "access_token": "access-token"
        }))
        .unwrap();

        assert_eq!(auth.access_token.as_deref(), Some("access-token"));
        assert!(auth.refresh_token.is_none());
    }

    #[test]
    fn grok_auth_allows_access_token_without_refresh_token() {
        let auth: GrokAuth = serde_json::from_value(json!({
            "access_token": "access-token"
        }))
        .unwrap();

        assert_eq!(auth.access_token.as_deref(), Some("access-token"));
        assert!(auth.refresh_token.is_none());
    }

    #[test]
    fn codex_config_allows_api_key_without_auth() {
        let config: CodexConfig = serde_json::from_value(json!({
            "models": ["codex-model"],
            "base_url": "https://api.example.com/v1",
            "api_key": "key"
        }))
        .unwrap();

        assert_eq!(config.base.api_key, "key");
        assert_eq!(config.base.base_url, "https://api.example.com/v1");
        assert!(matches!(config.auth, OneOrMany::Many(ref items) if items.is_empty()));
    }

    #[test]
    fn grok_config_allows_api_key_without_auth() {
        let config: GrokConfig = serde_json::from_value(json!({
            "models": ["grok-model"],
            "base_url": "https://api.example.com/v1",
            "api_key": "key"
        }))
        .unwrap();

        assert_eq!(config.base.api_key, "key");
        assert_eq!(config.base.base_url, "https://api.example.com/v1");
        assert!(matches!(config.auth, OneOrMany::Many(ref items) if items.is_empty()));
    }

    #[test]
    fn bun_config_provider_fields_can_be_ignored() {
        let config: Config = serde_json::from_value(json!({
            "model_priority": [
                "gemini_cli",
                "iflow",
                "openai_chat",
                "openai_responses",
                "qwen",
                "claude",
                "gemini",
                "codex",
                "grok"
            ],
            "gemini_cli": [{
                "models": ["gemini-2.5-pro"],
                "projects": ["project"],
                "auth": {
                    "access_token": "token",
                    "scope": "scope",
                    "token_type": "Bearer",
                    "expiry_date": 1,
                    "refresh_token": "refresh"
                }
            }],
            "qwen": [{
                "models": ["qwen"],
                "auth": {
                    "access_token": "token",
                    "refresh_token": "refresh",
                    "expiry_date": 1,
                    "status": "ok",
                    "token_type": "Bearer",
                    "expires_in": 1,
                    "scope": "scope",
                    "resource_url": "example.com"
                }
            }],
            "iflow": [{
                "models": ["iflow"],
                "auth": {
                    "access_token": "token",
                    "token_type": "Bearer",
                    "refresh_token": "refresh",
                    "expires_in": 1,
                    "scope": "scope",
                    "expiry_date": 1,
                    "userId": "user",
                    "userName": "name",
                    "avatar": "",
                    "email": null,
                    "phone": "",
                    "apiKey": "key"
                }
            }],
            "providers": {
                "openai_chat": [{
                    "models": ["gpt-4"],
                    "base_url": "https://api.openai.com/v1",
                    "api_key": "key"
                }]
            }
        }))
        .unwrap();

        assert_eq!(config.model_priority[0], "gemini_cli");
        assert_eq!(config.providers.openai_chat.len(), 1);
    }
}
