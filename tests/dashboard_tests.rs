use std::collections::HashMap;

use llm_proxy::app::dashboard::{
    DashboardAuthProvider, build_provider_models_endpoint, build_provider_responses_endpoint,
    resolve_auth_validation_concurrency, validation_auth_base_url, validation_request_headers,
};
use llm_proxy::provider::types::ProviderType;
use serde_json::json;

#[test]
fn provider_models_endpoint_keeps_explicit_models_path() {
    let endpoint = build_provider_models_endpoint("https://api.example.com/v1/models", "claude")
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
    let endpoint = build_provider_models_endpoint("https://api.example.com/anthropic", "claude")
        .expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/anthropic/v1/models");
}

#[test]
fn provider_models_endpoint_uses_models_for_versioned_non_openai_paths() {
    let endpoint = build_provider_models_endpoint("https://api.example.com/gemini/v1", "gemini")
        .expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/gemini/v1/models");
}

#[test]
fn provider_responses_endpoint_appends_responses_to_versioned_base_path() {
    let endpoint =
        build_provider_responses_endpoint("https://api.example.com/codex/v1").expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/codex/v1/responses");
}

#[test]
fn provider_responses_endpoint_keeps_explicit_responses_path() {
    let endpoint = build_provider_responses_endpoint("https://api.example.com/v1/responses")
        .expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/v1/responses");
}

#[test]
fn grok_validation_prefers_provider_base_url() {
    let provider = DashboardAuthProvider {
        enabled: true,
        base_url: "https://cli-chat-proxy.grok.com/v1".to_string(),
        headers: HashMap::new(),
        auth: None,
    };
    let auth = json!({
        "base_url": "https://api.x.ai/v1",
    });

    let base_url =
        validation_auth_base_url(ProviderType::Grok, &provider, &auth, "https://api.x.ai/v1");

    assert_eq!(base_url, "https://cli-chat-proxy.grok.com/v1");
}

#[test]
fn grok_validation_falls_back_to_auth_base_url() {
    let provider = DashboardAuthProvider {
        enabled: true,
        base_url: String::new(),
        headers: HashMap::new(),
        auth: None,
    };
    let auth = json!({
        "base_url": "https://cli-chat-proxy.grok.com/v1",
    });

    let base_url =
        validation_auth_base_url(ProviderType::Grok, &provider, &auth, "https://api.x.ai/v1");

    assert_eq!(base_url, "https://cli-chat-proxy.grok.com/v1");
}

#[test]
fn validation_headers_prefer_provider_over_auth_on_conflict() {
    let auth = json!({
        "account_id": "acct-1",
        "headers": {
            "X-Shared": "from-auth",
            "X-Auth-Only": "auth-value",
            "User-Agent": "auth-agent"
        }
    });
    let provider_headers = HashMap::from([
        ("X-Shared".to_string(), "from-provider".to_string()),
        ("X-Provider-Only".to_string(), "provider-value".to_string()),
    ]);

    let headers =
        validation_request_headers(ProviderType::Codex, "token-123", &auth, &provider_headers);

    assert_eq!(
        headers.get("authorization").map(String::as_str),
        Some("Bearer token-123")
    );
    assert_eq!(
        headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        headers.get("accept").map(String::as_str),
        Some("text/event-stream")
    );
    assert_eq!(
        headers.get("originator").map(String::as_str),
        Some("codex-tui")
    );
    assert_eq!(
        headers.get("chatgpt-account-id").map(String::as_str),
        Some("acct-1")
    );
    assert_eq!(
        headers.get("x-shared").map(String::as_str),
        Some("from-provider")
    );
    assert_eq!(
        headers.get("x-auth-only").map(String::as_str),
        Some("auth-value")
    );
    assert_eq!(
        headers.get("x-provider-only").map(String::as_str),
        Some("provider-value")
    );
    // Auth headers override fixed protocol headers when provider headers are absent.
    assert_eq!(
        headers.get("user-agent").map(String::as_str),
        Some("auth-agent")
    );
}

#[test]
fn validation_headers_for_grok_omit_codex_only_fields() {
    let auth = json!({
        "headers": {
            "X-Auth-Only": "auth-value"
        }
    });
    let provider_headers =
        HashMap::from([("X-Provider-Only".to_string(), "provider-value".to_string())]);

    let headers =
        validation_request_headers(ProviderType::Grok, "token-456", &auth, &provider_headers);

    assert_eq!(
        headers.get("authorization").map(String::as_str),
        Some("Bearer token-456")
    );
    assert_eq!(
        headers.get("x-auth-only").map(String::as_str),
        Some("auth-value")
    );
    assert_eq!(
        headers.get("x-provider-only").map(String::as_str),
        Some("provider-value")
    );
    assert!(!headers.contains_key("user-agent"));
    assert!(!headers.contains_key("originator"));
    assert!(!headers.contains_key("connection"));
    assert!(!headers.contains_key("chatgpt-account-id"));
}

#[test]
fn auth_validation_concurrency_defaults_and_keeps_requested_value() {
    assert_eq!(resolve_auth_validation_concurrency(None), 5);
    assert_eq!(resolve_auth_validation_concurrency(Some(0)), 5);
    assert_eq!(resolve_auth_validation_concurrency(Some(1)), 1);
    assert_eq!(resolve_auth_validation_concurrency(Some(8)), 8);
    assert_eq!(resolve_auth_validation_concurrency(Some(32)), 32);
    assert_eq!(resolve_auth_validation_concurrency(Some(100)), 100);
}
