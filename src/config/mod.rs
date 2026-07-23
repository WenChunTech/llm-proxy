use std::{
    collections::{HashMap, HashSet},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing_subscriber::EnvFilter;

use crate::{error::ProxyError, provider::types::ProviderType};

mod upstash;

pub use upstash::{UpstashRedis, parse_get_result, parse_set_result};

const DEFAULT_CONFIG_PATH: &str = "config.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub port: u16,
    pub api_key: Option<String>,
    pub log_level: Option<String>,
    pub model_priority: Vec<String>,
    pub fallback_models: Vec<String>,
    pub model_aliases: HashMap<String, String>,
    pub providers: ProviderGroups,
    pub retry: RetryConfig,
    /// Persist each request/response body under an ordered directory for debugging.
    pub debug_dump: DebugDumpConfig,
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
            model_aliases: HashMap::new(),
            providers: ProviderGroups::default(),
            retry: RetryConfig::default(),
            debug_dump: DebugDumpConfig::default(),
            extra: HashMap::new(),
        }
    }
}

/// When enabled, each proxied request is stored as:
/// `{dir}/{YYYYMMDD_HHMMSS_mmm}/` with `meta.json` + unconverted request/response bodies.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DebugDumpConfig {
    pub enabled: bool,
    /// Base directory for per-request dump folders. Default: `logs`.
    pub dir: String,
}

impl Default for DebugDumpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dir: "logs".to_string(),
        }
    }
}

impl Config {
    pub fn bind_addr(&self) -> String {
        format!("0.0.0.0:{}", self.port)
    }

    pub fn resolve_model_alias(&self, model: &str) -> String {
        self.model_aliases
            .get(model)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| model.to_string())
    }
}
fn parse_config_json(raw: &str) -> Result<Config, ProxyError> {
    let value: Value = serde_json::from_str(raw)?;
    parse_config_value(value)
}

pub fn parse_config_value(value: Value) -> Result<Config, ProxyError> {
    let explicit_port = value.get("port").is_some();
    let mut config: Config = serde_json::from_value(value.clone())?;
    // Drop legacy `server` blob from flatten extras so it is not rewritten on save.
    config.extra.remove("server");
    if !explicit_port
        && let Some(bind) = value
            .get("server")
            .and_then(|server| server.get("bind"))
            .and_then(Value::as_str)
        && let Some(port) = parse_bind_port(bind)
    {
        config.port = port;
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
    pub persist: ConfigPersist,
}

/// Where configuration is loaded from and written back to.
///
/// When Redis is configured: load from Redis only. If the key is missing, seed
/// Redis with default config and continue with Redis as the primary backend.
/// Without Redis: config file -> defaults. Writes always target the primary backend.
#[derive(Debug, Clone)]
pub enum ConfigPersist {
    Redis(UpstashRedis),
    File(PathBuf),
}

impl ConfigPersist {
    pub fn label(&self) -> String {
        match self {
            Self::Redis(redis) => format!("redis:{}", redis.key),
            Self::File(path) => path.display().to_string(),
        }
    }
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
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
    pub headers: Option<HashMap<String, String>>,
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
    pub headers: Option<HashMap<String, String>>,
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

pub async fn load_config() -> Result<LoadedConfig, ProxyError> {
    load_config_from_sources(ConfigSources {
        cli_path: cli_config_path(),
        redis: UpstashRedis::from_env()?,
        default_path: PathBuf::from(DEFAULT_CONFIG_PATH),
    })
    .await
}

pub async fn load_config_from_sources(sources: ConfigSources) -> Result<LoadedConfig, ProxyError> {
    let file_path = sources.cli_path.unwrap_or(sources.default_path);

    // When Redis is configured it is exclusive: never fall back to the config file.
    if let Some(redis) = sources.redis {
        if let Some(raw) = redis.get().await? {
            let config = parse_config_json(&raw)?;
            validate_config(&config)?;
            return Ok(LoadedConfig {
                config,
                persist: ConfigPersist::Redis(redis),
            });
        }

        // Key miss: initialize with defaults and seed Redis so the key exists.
        let config = Config::default();
        validate_config(&config)?;
        let raw = serde_json::to_string_pretty(&config)?;
        redis.set(&raw).await?;
        return Ok(LoadedConfig {
            config,
            persist: ConfigPersist::Redis(redis),
        });
    }

    let config = read_optional_config_file(&file_path)?.unwrap_or_default();
    validate_config(&config)?;
    Ok(LoadedConfig {
        config,
        persist: ConfigPersist::File(file_path),
    })
}

pub struct ConfigSources {
    pub cli_path: Option<PathBuf>,
    pub redis: Option<UpstashRedis>,
    pub default_path: PathBuf,
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

    let configured_models: HashSet<String> = config
        .providers
        .iter_configs()
        .into_iter()
        .flat_map(|(_, _, provider)| provider.models().to_vec())
        .collect();
    let mut seen_aliases = HashSet::new();
    for (alias, target) in &config.model_aliases {
        let alias = alias.trim();
        let target = target.trim();
        if alias.is_empty() {
            return Err(ProxyError::Config(
                "model alias name must not be empty".to_string(),
            ));
        }
        if target.is_empty() {
            return Err(ProxyError::Config(format!(
                "model alias '{alias}' target must not be empty"
            )));
        }
        if !seen_aliases.insert(alias.to_string()) {
            return Err(ProxyError::Config(format!(
                "duplicate model alias: {alias}"
            )));
        }
        if alias == target {
            return Err(ProxyError::Config(format!(
                "model alias '{alias}' must not target itself"
            )));
        }
        if !configured_models.contains(alias) {
            return Err(ProxyError::Config(format!(
                "model alias '{alias}' is not in any provider models"
            )));
        }
        if !configured_models.contains(target) {
            return Err(ProxyError::Config(format!(
                "model alias '{alias}' target '{target}' is not in any provider models"
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

fn read_optional_config_file(path: &Path) -> Result<Option<Config>, ProxyError> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    parse_config_json(&raw).map(Some)
}

pub async fn persist_config(persist: &ConfigPersist, config: &Config) -> Result<(), ProxyError> {
    let raw = serde_json::to_string_pretty(config)?;
    match persist {
        ConfigPersist::Redis(redis) => redis.set(&raw).await,
        ConfigPersist::File(path) => {
            if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, format!("{raw}\n"))?;
            Ok(())
        }
    }
}
