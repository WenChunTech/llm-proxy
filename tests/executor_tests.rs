use llm_proxy::config::{
    BaseProviderConfig, CodexConfig, OneOrMany, OpenAiChatConfig, OpenAiResponsesConfig,
    ProviderConfig,
};
use llm_proxy::error::ProxyError;
use llm_proxy::provider::credentials::{
    coverage_attempt_budget, credential_slot_count, provider_test_slot_count,
};
use llm_proxy::provider::executor::{
    clean_image_base_url, image_provider_config, prepare_targets, rotate_attempt_targets,
    should_attempt,
};
use llm_proxy::provider::types::{AttemptTarget, ProviderType};
use llm_proxy::state::ProviderCursor;
use serde_json::json;

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
    // Model testing with api_key only exercises the api_key itself.
    assert_eq!(provider_test_slot_count(&target), 1);
    assert_eq!(coverage_attempt_budget(&[target], 1), 3);
}

#[test]
fn provider_test_slot_count_covers_all_auth_when_api_key_missing() {
    let base = BaseProviderConfig {
        enabled: true,
        models: vec!["m".to_string()],
        base_url: "https://codex.example/v1".to_string(),
        api_key: String::new(),
        headers: Default::default(),
    };
    let target = AttemptTarget {
        provider_type: ProviderType::Codex,
        provider_index: 0,
        config_index: 0,
        config: ProviderConfig::Codex(CodexConfig {
            base,
            auth: serde_json::from_value(serde_json::json!([
                { "access_token": "a" },
                { "access_token": "b" },
                { "access_token": "c", "disabled": true }
            ]))
            .unwrap(),
        }),
    };

    assert_eq!(credential_slot_count(&target), 2);
    assert_eq!(provider_test_slot_count(&target), 2);
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

#[test]
fn prepare_targets_converts_once_per_provider_type() {
    let targets = vec![
        target(ProviderType::Chat, 0),
        target(ProviderType::Chat, 1),
        target(ProviderType::Responses, 0),
    ];
    let body = json!({ "model": "m", "messages": [] });
    let mut conversions = Vec::new();
    let prepared = prepare_targets(targets, "m", &body, |provider_type, body| {
        conversions.push(provider_type);
        Ok(body)
    });

    assert_eq!(
        conversions,
        vec![ProviderType::Chat, ProviderType::Responses]
    );
    assert_eq!(prepared.targets.len(), 3);
    assert_eq!(prepared.prepared.len(), 2);
    assert!(prepared.dropped.is_empty());
    assert!(prepared.first_error.is_none());
}

#[test]
fn prepare_targets_drops_targets_whose_conversion_fails() {
    let targets = vec![
        target_with_base_url(ProviderType::Responses, 1, "https://a.example/v1"),
        target_with_base_url(ProviderType::Responses, 3, "https://b.example/v1"),
        target(ProviderType::Chat, 5),
    ];
    let body = json!({ "model": "m" });
    let prepared = prepare_targets(targets, "m", &body, |provider_type, _| {
        if provider_type == ProviderType::Responses {
            Err(ProxyError::InvalidRequest("schema mismatch".to_string()))
        } else {
            Ok(json!({ "converted": true }))
        }
    });

    assert_eq!(prepared.targets.len(), 1);
    assert_eq!(prepared.targets[0].provider_type, ProviderType::Chat);
    assert_eq!(prepared.dropped.len(), 2);
    assert!(
        prepared
            .dropped
            .iter()
            .all(|record| record.provider_type == ProviderType::Responses)
    );
    assert_eq!(prepared.dropped[0].config_index, 1);
    assert_eq!(prepared.dropped[1].config_index, 3);
    assert!(prepared.dropped.iter().all(|record| record.error.is_some()));
    assert!(prepared.first_error.is_some());
}

#[test]
fn prepare_targets_reports_first_error_when_every_conversion_fails() {
    let targets = vec![
        target(ProviderType::Chat, 0),
        target(ProviderType::Responses, 1),
    ];
    let body = json!({ "model": "m" });
    let prepared = prepare_targets(targets, "m", &body, |_, _| {
        Err(ProxyError::InvalidRequest(
            "request body must be a JSON object".to_string(),
        ))
    });

    assert!(prepared.targets.is_empty());
    assert!(prepared.prepared.is_empty());
    assert_eq!(prepared.dropped.len(), 2);
    assert!(prepared.first_error.is_some());
}
