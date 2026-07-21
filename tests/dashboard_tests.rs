use llm_proxy::app::dashboard::{
    DashboardAuthProvider, build_provider_models_endpoint, build_provider_responses_endpoint,
    validation_auth_base_url,
};
use llm_proxy::provider::types::ProviderType;
use serde_json::json;

#[test]
fn provider_models_endpoint_keeps_explicit_models_path() {
    let endpoint =
        build_provider_models_endpoint("https://api.example.com/v1/models", "claude")
            .expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/v1/models");
}

#[test]
fn provider_models_endpoint_uses_openai_versioned_base_path() {
    let endpoint =
        build_provider_models_endpoint("https://api.example.com/openai/v1", "openai_chat")
            .expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/openai/v1/models");
}

#[test]
fn provider_models_endpoint_adds_v1_for_non_openai_roots() {
    let endpoint =
        build_provider_models_endpoint("https://api.example.com/anthropic", "claude")
            .expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/anthropic/v1/models");
}

#[test]
fn provider_models_endpoint_uses_models_for_versioned_non_openai_paths() {
    let endpoint =
        build_provider_models_endpoint("https://api.example.com/gemini/v1", "gemini")
            .expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/gemini/v1/models");
}

#[test]
fn provider_responses_endpoint_appends_responses_to_versioned_base_path() {
    let endpoint = build_provider_responses_endpoint("https://api.example.com/codex/v1")
        .expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/codex/v1/responses");
}

#[test]
fn provider_responses_endpoint_keeps_explicit_responses_path() {
    let endpoint = build_provider_responses_endpoint("https://api.example.com/v1/responses")
        .expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/v1/responses");
}

#[test]
fn grok_validation_prefers_auth_base_url() {
    let provider = DashboardAuthProvider {
        enabled: true,
        base_url: "https://api.x.ai/v1".to_string(),
        auth: None,
    };
    let auth = json!({
        "base_url": "https://cli-chat-proxy.grok.com/v1",
    });

    let base_url =
        validation_auth_base_url(ProviderType::Grok, &provider, &auth, "https://api.x.ai/v1");

    assert_eq!(base_url, "https://cli-chat-proxy.grok.com/v1");
}

