use llm_proxy::config::{
    BaseProviderConfig, CodexConfig, OneOrMany, OpenAiChatConfig, OpenAiResponsesConfig,
    ProviderConfig,
};
use llm_proxy::provider::credentials::{coverage_attempt_budget, credential_slot_count};
use llm_proxy::provider::executor::{
    clean_image_base_url, image_provider_config, rotate_attempt_targets, should_attempt,
};
use llm_proxy::provider::types::{AttemptTarget, ProviderType};
use llm_proxy::state::ProviderCursor;

#[test]
fn rotate_attempt_targets_starts_with_provider_cursor() {
    let mut targets = vec![
        target_with_base_url(ProviderType::Chat, 0, "https://first.example/v1"),
        target_with_base_url(ProviderType::Chat, 1, "https://second.example/v1"),
        target_with_base_url(ProviderType::Responses, 0, "https://responses.example/v1"),
    ];

    rotate_attempt_targets(
        &mut targets,
        Some(ProviderCursor {
            provider_type: ProviderType::Chat,
            base_url: "https://second.example/v1".to_string(),
            config_index: 1,
        }),
    );

    assert_eq!(
        target_order(&targets),
        vec![
            (ProviderType::Chat, 1),
            (ProviderType::Responses, 0),
            (ProviderType::Chat, 0),
        ]
    );
}

#[test]
fn rotate_attempt_targets_keeps_default_order_when_cursor_is_missing() {
    let mut targets = vec![
        target(ProviderType::Chat, 0),
        target(ProviderType::Responses, 0),
    ];

    rotate_attempt_targets(
        &mut targets,
        Some(ProviderCursor {
            provider_type: ProviderType::Claude,
            base_url: "https://claude.example/v1".to_string(),
            config_index: 0,
        }),
    );

    assert_eq!(
        target_order(&targets),
        vec![(ProviderType::Chat, 0), (ProviderType::Responses, 0)]
    );
}

#[test]
fn rotate_attempt_targets_requires_matching_base_url() {
    let mut targets = vec![
        target_with_base_url(ProviderType::Chat, 0, "https://first.example/v1"),
        target_with_base_url(ProviderType::Chat, 1, "https://second.example/v1"),
    ];

    rotate_attempt_targets(
        &mut targets,
        Some(ProviderCursor {
            provider_type: ProviderType::Chat,
            base_url: "https://other.example/v1".to_string(),
            config_index: 1,
        }),
    );

    assert_eq!(
        target_order(&targets),
        vec![(ProviderType::Chat, 0), (ProviderType::Chat, 1)]
    );
}

#[test]
fn image_provider_config_uses_api_key_provider_configs() {
    let target = target(ProviderType::Chat, 0);
    let config = image_provider_config(&target).expect("image config");

    assert_eq!(config.base_url, "https://openai.example/v1");
    assert_eq!(config.api_key, "key");
}

#[test]
fn image_provider_config_excludes_oauth_provider_configs() {
    let base = BaseProviderConfig {
        enabled: true,
        models: vec!["m".to_string()],
        base_url: String::new(),
        api_key: String::new(),
        headers: Default::default(),
    };
    let target = AttemptTarget {
        provider_type: ProviderType::Codex,
        provider_index: 0,
        config_index: 0,
        config: ProviderConfig::Codex(CodexConfig {
            base,
            auth: OneOrMany::Many(Vec::new()),
        }),
    };

    assert!(image_provider_config(&target).is_none());
}

#[test]
fn clean_image_base_url_removes_trailing_v1() {
    assert_eq!(
        clean_image_base_url("https://api.example.com/v1/"),
        "https://api.example.com"
    );
    assert_eq!(
        clean_image_base_url("https://api.example.com/custom"),
        "https://api.example.com/custom"
    );
}

#[test]
fn should_attempt_stops_at_max_retries() {
    assert!(should_attempt(0, 1));
    assert!(should_attempt(4, 5));
    assert!(!should_attempt(5, 5));
    assert!(!should_attempt(0, 0));
}

#[test]
fn credential_slot_count_is_one_for_non_oauth_providers() {
    assert_eq!(credential_slot_count(&target(ProviderType::Chat, 0)), 1);
    assert_eq!(
        credential_slot_count(&target(ProviderType::Responses, 0)),
        1
    );
}

#[test]
fn coverage_attempt_budget_covers_all_auth_slots_beyond_max_retries() {
    let base = BaseProviderConfig {
        enabled: true,
        models: vec!["m".to_string()],
        base_url: "https://grok.example/v1".to_string(),
        api_key: "sk-key".to_string(),
        headers: Default::default(),
    };
    let target = AttemptTarget {
        provider_type: ProviderType::Grok,
        provider_index: 0,
        config_index: 0,
        config: ProviderConfig::Grok(llm_proxy::config::GrokConfig {
            base,
            auth: serde_json::from_value(serde_json::json!([
                { "access_token": "a" },
                { "access_token": "b", "disabled": true },
                { "access_token": "c" }
            ]))
            .unwrap(),
        }),
    };

    // api_key + 2 enabled auths = 3 slots; disabled auth ignored.
    assert_eq!(credential_slot_count(&target), 3);
    assert_eq!(coverage_attempt_budget(&[target], 1), 3);
}

#[test]
fn coverage_attempt_budget_uses_max_retries_when_higher() {
    let targets = vec![
        target(ProviderType::Chat, 0),
        target(ProviderType::Responses, 0),
    ];
    assert_eq!(coverage_attempt_budget(&targets, 5), 5);
    assert_eq!(coverage_attempt_budget(&targets, 1), 2);
}

fn target(provider_type: ProviderType, config_index: usize) -> AttemptTarget {
    target_with_base_url(provider_type, config_index, "https://openai.example/v1")
}

fn target_with_base_url(
    provider_type: ProviderType,
    config_index: usize,
    base_url: &str,
) -> AttemptTarget {
    let base = BaseProviderConfig {
        enabled: true,
        models: vec!["m".to_string()],
        base_url: base_url.to_string(),
        api_key: "key".to_string(),
        headers: Default::default(),
    };
    let config = match provider_type {
        ProviderType::Chat => ProviderConfig::OpenAiChat(OpenAiChatConfig { base }),
        ProviderType::Responses => ProviderConfig::OpenAiResponses(OpenAiResponsesConfig { base }),
        other => panic!("unsupported test provider: {other:?}"),
    };
    AttemptTarget {
        provider_type,
        provider_index: config_index,
        config_index,
        config,
    }
}

fn target_order(targets: &[AttemptTarget]) -> Vec<(ProviderType, usize)> {
    targets
        .iter()
        .map(|target| (target.provider_type, target.config_index))
        .collect()
}
