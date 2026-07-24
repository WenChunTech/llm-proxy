use std::sync::Arc;

use llm_proxy::config::{
    BaseProviderConfig, Config, GeminiConfig, OpenAiChatConfig, ProviderGroups,
};
use llm_proxy::provider::registry::ProviderRegistry;
use llm_proxy::provider::types::ProviderType;

fn base(models: &[&str], base_url: &str) -> BaseProviderConfig {
    BaseProviderConfig {
        enabled: true,
        models: models.iter().map(|m| (*m).to_string()).collect(),
        base_url: base_url.to_string(),
        api_key: "key".to_string(),
        headers: Default::default(),
    }
}

#[test]
fn providers_are_sorted_by_priority() {
    let config = Config {
        model_priority: vec!["openai_chat".into(), "gemini".into()],
        providers: ProviderGroups {
            openai_chat: vec![OpenAiChatConfig {
                base: base(&["m"], "https://openai.example/v1"),
            }],
            gemini: vec![GeminiConfig {
                base: base(&["m"], "https://gemini.example"),
            }],
            ..Default::default()
        },
        ..Default::default()
    };

    let registry = ProviderRegistry::new(Arc::new(config));
    assert_eq!(
        registry.providers_for_model("m"),
        vec![ProviderType::Chat, ProviderType::Gemini]
    );
}

#[test]
fn unsupported_bun_priority_entries_are_ignored() {
    let config = Config {
        model_priority: vec![
            "gemini_cli".into(),
            "iflow".into(),
            "openai_responses".into(),
            "qwen".into(),
            "openai_chat".into(),
        ],
        providers: ProviderGroups {
            openai_chat: vec![OpenAiChatConfig {
                base: base(&["m"], "https://openai.example/v1"),
            }],
            ..Default::default()
        },
        ..Default::default()
    };

    let registry = ProviderRegistry::new(Arc::new(config));
    assert_eq!(registry.providers_for_model("m"), vec![ProviderType::Chat]);
}
