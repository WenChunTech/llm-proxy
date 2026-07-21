use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    config::Config,
    provider::types::{AttemptTarget, ProviderType},
};

#[derive(Clone)]
pub struct ProviderRegistry {
    config: Arc<Config>,
    model_index: HashMap<String, Vec<ProviderType>>,
}

impl ProviderRegistry {
    pub fn new(config: Arc<Config>) -> Self {
        let model_index = build_model_index(&config);
        Self {
            config,
            model_index,
        }
    }

    pub fn providers_for_model(&self, model: &str) -> Vec<ProviderType> {
        let configured = self.model_index.get(model).cloned().unwrap_or_default();
        let mut remaining: HashSet<_> = configured.iter().copied().collect();
        let mut ordered = Vec::new();

        for provider_type in self.priority() {
            if remaining.remove(&provider_type) {
                ordered.push(provider_type);
            }
        }

        for provider_type in configured {
            if remaining.remove(&provider_type) {
                ordered.push(provider_type);
            }
        }

        ordered
    }

    pub fn attempt_targets(&self, model: &str) -> Vec<AttemptTarget> {
        let mut targets = Vec::new();
        for (provider_index, provider_type) in
            self.providers_for_model(model).into_iter().enumerate()
        {
            for (config_index, config) in self
                .config
                .providers
                .configs_for(provider_type)
                .into_iter()
                .enumerate()
            {
                if config.enabled() && config.models().iter().any(|m| m == model) {
                    targets.push(AttemptTarget {
                        provider_type,
                        provider_index,
                        config_index,
                        config,
                    });
                }
            }
        }
        targets
    }

    pub fn has_any_provider_for_model(&self, model: &str) -> bool {
        self.model_index
            .get(model)
            .is_some_and(|items| !items.is_empty())
    }

    pub fn configured_models(&self) -> Vec<(String, ProviderType)> {
        let mut models = Vec::new();
        for (model, providers) in &self.model_index {
            for provider in providers {
                models.push((model.clone(), *provider));
            }
        }
        models.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.as_str().cmp(b.1.as_str())));
        models
    }

    fn priority(&self) -> Vec<ProviderType> {
        if self.config.model_priority.is_empty() {
            return ProviderType::default_priority().to_vec();
        }

        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for provider_id in &self.config.model_priority {
            let Some(provider_type) = ProviderType::from_config_id(provider_id) else {
                continue;
            };
            if seen.insert(provider_type) {
                result.push(provider_type);
            }
        }
        for provider_type in ProviderType::default_priority() {
            if seen.insert(*provider_type) {
                result.push(*provider_type);
            }
        }
        result
    }
}

fn build_model_index(config: &Config) -> HashMap<String, Vec<ProviderType>> {
    let mut index: HashMap<String, Vec<ProviderType>> = HashMap::new();
    let mut add = |provider_type: ProviderType, base: &crate::config::BaseProviderConfig| {
        if !base.enabled {
            return;
        }
        for model in &base.models {
            let providers = index.entry(model.clone()).or_default();
            if !providers.contains(&provider_type) {
                providers.push(provider_type);
            }
        }
    };

    for (provider_type, _, config) in config.providers.iter_configs() {
        add(provider_type, config.base());
    }

    index
}
