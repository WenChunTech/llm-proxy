use serde_json::{Value, json};

use crate::{
    config::{BaseProviderConfig, Config, OneOrMany, ProviderConfig},
    error::ProxyError,
    provider::types::ProviderType,
    state::AppSnapshot,
};

use super::types::{DashboardConfig, DashboardPayload, DashboardProvider, DashboardRetry};

pub(super) fn config_payload(snapshot: &AppSnapshot) -> DashboardPayload {
    DashboardPayload {
        port: snapshot.config.port,
        providers: dashboard_providers(&snapshot.config),
        model_priority: snapshot.config.model_priority.clone(),
        fallback_models: snapshot.config.fallback_models.clone(),
        model_aliases: snapshot.config.model_aliases.clone(),
        retry: DashboardRetry {
            max_retries: snapshot.config.retry.max_retries,
            backoff_step_ms: snapshot.config.retry.backoff_step_ms,
        },
        api_key: snapshot.config.api_key.clone().unwrap_or_default(),
        api_key_enabled: snapshot
            .config
            .api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty()),
    }
}

pub(crate) fn models_payload(snapshot: &AppSnapshot) -> Value {
    let mut data: Vec<Value> = snapshot
        .registry
        .configured_models()
        .into_iter()
        .map(|(id, provider)| {
            json!({
                "id": id,
                "object": "model",
                "owned_by": provider.as_str()
            })
        })
        .collect();
    let mut aliases: Vec<_> = snapshot.config.model_aliases.iter().collect();
    aliases.sort_by(|a, b| a.0.cmp(b.0));
    for (alias, target) in aliases {
        let alias = alias.trim();
        let target = target.trim();
        if alias.is_empty() || target.is_empty() {
            continue;
        }
        data.push(json!({
            "id": alias,
            "object": "model",
            "owned_by": "alias",
            "root": target
        }));
    }
    json!({ "object": "list", "data": data })
}


fn dashboard_providers(config: &Config) -> Vec<DashboardProvider> {
    let mut providers = Vec::new();
    for (provider_type, index, provider_config) in config.providers.iter_configs() {
        providers.push(dashboard_provider_from_config(
            provider_type,
            index,
            &provider_config,
        ));
    }
    providers
}

fn dashboard_provider_from_config(
    kind: ProviderType,
    index: usize,
    config: &ProviderConfig,
) -> DashboardProvider {
    match config {
        ProviderConfig::OpenAiChat(config) => provider_payload(kind, index, &config.base, None),
        ProviderConfig::OpenAiResponses(config) => {
            provider_payload(kind, index, &config.base, None)
        }
        ProviderConfig::Claude(config) => provider_payload(kind, index, &config.base, None),
        ProviderConfig::Gemini(config) => provider_payload(kind, index, &config.base, None),
        ProviderConfig::Codex(config) => provider_payload(
            kind,
            index,
            &config.base,
            serde_json::to_value(&config.auth).ok(),
        ),
        ProviderConfig::Grok(config) => provider_payload(
            kind,
            index,
            &config.base,
            serde_json::to_value(&config.auth).ok(),
        ),
    }
}

fn provider_payload(
    kind: ProviderType,
    index: usize,
    base: &BaseProviderConfig,
    auth_json: Option<Value>,
) -> DashboardProvider {
    DashboardProvider {
        id: format!("{}:{index}", kind.as_str()),
        kind: kind.as_str().to_string(),
        name: provider_name(kind, &base.base_url, index),
        enabled: base.enabled,
        base_url: base.base_url.clone(),
        api_key: base.api_key.clone(),
        models: base.models.clone(),
        headers: base.headers.clone(),
        auth: auth_json,
    }
}

fn provider_name(kind: ProviderType, base_url: &str, index: usize) -> String {
    let label = kind.display_name();
    if base_url.trim().is_empty() {
        format!("{label} #{index}", index = index + 1)
    } else {
        format!("{label} - {base_url}")
    }
}

impl DashboardConfig {
    pub(super) fn apply_to(self, mut config: Config) -> Config {
        if let Some(port) = self.port {
            config.port = port;
        }
        config.model_priority = self.model_priority;
        config.fallback_models = self.fallback_models;
        config.model_aliases = self.model_aliases;
        config.api_key = self
            .api_key
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        config.retry.max_retries = self.retry.max_retries;
        config.retry.backoff_step_ms = self.retry.backoff_step_ms;

        let providers = self.providers.into_iter().filter_map(|provider| {
            match provider.persisted_provider_config() {
                Ok(Some(config)) => Some(config),
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!(error = %error, "invalid dashboard provider config skipped");
                    None
                }
            }
        });
        config.providers = crate::config::ProviderGroups::from_configs(providers);
        config
    }
}

impl DashboardProvider {
    fn base(&self) -> BaseProviderConfig {
        BaseProviderConfig {
            enabled: self.enabled,
            models: self.models.clone(),
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            headers: self.headers.clone(),
        }
    }

    pub(super) fn provider_type(&self) -> Result<ProviderType, ProxyError> {
        ProviderType::from_config_id(&self.kind).ok_or_else(|| {
            ProxyError::InvalidRequest(format!("unsupported provider kind: {}", self.kind))
        })
    }

    pub(super) fn provider_config(&self) -> Result<ProviderConfig, ProxyError> {
        self.provider_type().and_then(|kind| match kind {
            ProviderType::Chat => Ok(ProviderConfig::OpenAiChat(
                crate::config::OpenAiChatConfig { base: self.base() },
            )),
            ProviderType::Responses => Ok(ProviderConfig::OpenAiResponses(
                crate::config::OpenAiResponsesConfig { base: self.base() },
            )),
            ProviderType::Claude => Ok(ProviderConfig::Claude(crate::config::ClaudeConfig {
                base: self.base(),
            })),
            ProviderType::Gemini => Ok(ProviderConfig::Gemini(crate::config::GeminiConfig {
                base: self.base(),
            })),
            ProviderType::Codex => {
                self.codex_config()?
                    .map(ProviderConfig::Codex)
                    .ok_or_else(|| {
                        ProxyError::InvalidRequest("Codex auth JSON is required".to_string())
                    })
            }
            ProviderType::Grok => self
                .grok_config()?
                .map(ProviderConfig::Grok)
                .ok_or_else(|| {
                    ProxyError::InvalidRequest("Grok auth JSON is required".to_string())
                }),
        })
    }

    fn persisted_provider_config(&self) -> Result<Option<ProviderConfig>, ProxyError> {
        self.provider_type().and_then(|kind| match kind {
            ProviderType::Chat => Ok(Some(ProviderConfig::OpenAiChat(
                crate::config::OpenAiChatConfig { base: self.base() },
            ))),
            ProviderType::Responses => Ok(Some(ProviderConfig::OpenAiResponses(
                crate::config::OpenAiResponsesConfig { base: self.base() },
            ))),
            ProviderType::Claude => Ok(Some(ProviderConfig::Claude(crate::config::ClaudeConfig {
                base: self.base(),
            }))),
            ProviderType::Gemini => Ok(Some(ProviderConfig::Gemini(crate::config::GeminiConfig {
                base: self.base(),
            }))),
            ProviderType::Codex => self
                .codex_config()
                .map(|config| config.map(ProviderConfig::Codex)),
            ProviderType::Grok => self
                .grok_config()
                .map(|config| config.map(ProviderConfig::Grok)),
        })
    }

    fn codex_config(&self) -> Result<Option<crate::config::CodexConfig>, ProxyError> {
        let auth_config = match self.auth.as_ref() {
            Some(auth_json) => parse_auth_value(auth_json.clone())?,
            None if self.api_key.trim().is_empty() => return Ok(None),
            None => OneOrMany::Many(Vec::new()),
        };
        Ok(Some(crate::config::CodexConfig {
            base: self.base(),
            auth: auth_config,
        }))
    }

    fn grok_config(&self) -> Result<Option<crate::config::GrokConfig>, ProxyError> {
        let auth_config = match self.auth.as_ref() {
            Some(auth_json) => parse_auth_value(auth_json.clone())?,
            None if self.api_key.trim().is_empty() => return Ok(None),
            None => OneOrMany::Many(Vec::new()),
        };
        Ok(Some(crate::config::GrokConfig {
            base: self.base(),
            auth: auth_config,
        }))
    }
}

fn parse_auth_value<T>(value: Value) -> Result<OneOrMany<T>, ProxyError>
where
    T: serde::de::DeserializeOwned,
{
    if value.is_array() {
        serde_json::from_value::<Vec<T>>(value)
            .map(OneOrMany::Many)
            .map_err(ProxyError::from)
    } else if value.is_object() {
        serde_json::from_value::<T>(value)
            .map(OneOrMany::One)
            .map_err(ProxyError::from)
    } else {
        Err(ProxyError::InvalidRequest(
            "auth must be a JSON object or array".to_string(),
        ))
    }
}

