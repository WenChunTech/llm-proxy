use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::{
    config::{CodexAuth, Config, GrokAuth, validate_config},
    error::ProxyError,
    provider::{Providers, registry::ProviderRegistry, types::ProviderType},
};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<tokio::sync::RwLock<AppStateInner>>,
    cursors: Arc<tokio::sync::RwLock<SuccessCursors>>,
    auth_cache: Arc<tokio::sync::RwLock<RuntimeAuthCache>>,
    pub providers: Providers,
    pub http: reqwest::Client,
}

#[derive(Clone)]
struct AppStateInner {
    config: Arc<Config>,
    registry: ProviderRegistry,
    config_path: Option<PathBuf>,
}

#[derive(Clone)]
pub struct AppSnapshot {
    pub config: Arc<Config>,
    pub registry: ProviderRegistry,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderCursor {
    pub provider_type: ProviderType,
    pub base_url: String,
    pub config_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthCursorKey {
    pub provider_type: ProviderType,
    pub base_url: String,
    pub config_index: usize,
}

#[derive(Debug, Default)]
struct SuccessCursors {
    providers: HashMap<String, ProviderCursor>,
    auth: HashMap<AuthCursorKey, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AuthCacheKey {
    provider_type: ProviderType,
    config_index: usize,
    auth_index: usize,
}

#[derive(Debug, Default)]
struct RuntimeAuthCache {
    codex: HashMap<AuthCacheKey, CodexAuth>,
    grok: HashMap<AuthCacheKey, GrokAuth>,
}

impl AppState {
    pub fn from_loaded(loaded: crate::config::LoadedConfig) -> Result<Self, ProxyError> {
        let config = Arc::new(loaded.config);
        let registry = ProviderRegistry::new(config.clone());
        let http = reqwest::Client::builder().build()?;
        tracing::info!(
            config_source = config_source(loaded.source_path.as_ref()),
            port = config.port,
            bind = %config.bind_addr(),
            log_level = config.log_level.as_deref().unwrap_or("info"),
            provider_configs = config.providers.iter_configs().len(),
            configured_models = registry.configured_models().len(),
            "configuration loaded"
        );
        Ok(Self {
            inner: Arc::new(tokio::sync::RwLock::new(AppStateInner {
                config,
                registry,
                config_path: loaded.source_path,
            })),
            cursors: Arc::new(tokio::sync::RwLock::new(SuccessCursors::default())),
            auth_cache: Arc::new(tokio::sync::RwLock::new(RuntimeAuthCache::default())),
            providers: Providers::new(),
            http,
        })
    }

    pub async fn snapshot(&self) -> AppSnapshot {
        let inner = self.inner.read().await;
        AppSnapshot {
            config: inner.config.clone(),
            registry: inner.registry.clone(),
        }
    }

    pub async fn bind_addr(&self) -> String {
        self.snapshot().await.config.bind_addr()
    }

    pub async fn update_config(&self, config: Config) -> Result<(), ProxyError> {
        validate_config(&config)?;
        let raw = serde_json::to_string_pretty(&config)?;
        let mut inner = self.inner.write().await;
        let path = inner
            .config_path
            .clone()
            .ok_or_else(|| ProxyError::Config("config was loaded from environment".to_string()))?;
        if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, format!("{raw}\n"))?;
        let config = Arc::new(config);
        inner.registry = ProviderRegistry::new(config.clone());
        inner.config = config;
        self.auth_cache.write().await.clear();
        tracing::info!(
            config_path = %path.display(),
            port = inner.config.port,
            bind = %inner.config.bind_addr(),
            log_level = inner.config.log_level.as_deref().unwrap_or("info"),
            provider_configs = inner.config.providers.iter_configs().len(),
            configured_models = inner.registry.configured_models().len(),
            "configuration updated"
        );
        Ok(())
    }

    pub async fn provider_cursor(&self, model: &str) -> Option<ProviderCursor> {
        self.cursors.read().await.providers.get(model).cloned()
    }

    pub async fn record_provider_success(&self, model: &str, cursor: ProviderCursor) {
        self.cursors
            .write()
            .await
            .providers
            .insert(model.to_string(), cursor);
    }

    pub async fn auth_cursor(&self, key: &AuthCursorKey) -> Option<usize> {
        self.cursors.read().await.auth.get(key).copied()
    }

    pub async fn record_auth_success(&self, key: AuthCursorKey, auth_index: usize) {
        self.cursors.write().await.auth.insert(key, auth_index);
    }

    pub async fn cached_codex_auth(
        &self,
        key: &AuthCursorKey,
        auth_index: usize,
    ) -> Option<CodexAuth> {
        let cache_key = auth_cache_key(key, auth_index);
        self.auth_cache.read().await.codex.get(&cache_key).cloned()
    }

    pub async fn record_codex_auth(&self, key: &AuthCursorKey, auth_index: usize, auth: CodexAuth) {
        let cache_key = auth_cache_key(key, auth_index);
        self.auth_cache.write().await.codex.insert(cache_key, auth);
    }

    pub async fn cached_grok_auth(
        &self,
        key: &AuthCursorKey,
        auth_index: usize,
    ) -> Option<GrokAuth> {
        let cache_key = auth_cache_key(key, auth_index);
        self.auth_cache.read().await.grok.get(&cache_key).cloned()
    }

    pub async fn record_grok_auth(&self, key: &AuthCursorKey, auth_index: usize, auth: GrokAuth) {
        let cache_key = auth_cache_key(key, auth_index);
        self.auth_cache.write().await.grok.insert(cache_key, auth);
    }
}

fn config_source(path: Option<&PathBuf>) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_else(|| "environment".to_string())
}

impl RuntimeAuthCache {
    fn clear(&mut self) {
        self.codex.clear();
        self.grok.clear();
    }
}

fn auth_cache_key(key: &AuthCursorKey, auth_index: usize) -> AuthCacheKey {
    AuthCacheKey {
        provider_type: key.provider_type,
        config_index: key.config_index,
        auth_index,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use crate::config::{Config, LoadedConfig};

    use super::*;

    #[tokio::test]
    async fn update_config_creates_missing_config_file_and_updates_runtime_state() {
        let path = unique_config_path("update-config-creates-missing-file");
        let parent = path.parent().unwrap().to_path_buf();
        let state = AppState::from_loaded(LoadedConfig {
            config: Config::default(),
            source_path: Some(path.clone()),
        })
        .unwrap();
        let next = Config {
            port: 4567,
            api_key: Some("proxy-key".to_string()),
            ..Default::default()
        };

        state.update_config(next).await.unwrap();

        let saved: Config = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let snapshot = state.snapshot().await;
        assert_eq!(saved.port, 4567);
        assert_eq!(saved.api_key.as_deref(), Some("proxy-key"));
        assert_eq!(snapshot.config.port, 4567);
        assert_eq!(snapshot.config.api_key.as_deref(), Some("proxy-key"));

        let _ = fs::remove_dir_all(parent);
    }

    fn unique_config_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("llm-proxy-{name}-{}-{nanos}", std::process::id()))
            .join("config.json")
    }
}
