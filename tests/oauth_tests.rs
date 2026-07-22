use llm_proxy::config::{CodexAuth, GrokAuth, OneOrMany};
use llm_proxy::provider::oauth::{
    SelectedCredential, codex_access_token, codex_oauth_base_url, current_millis,
    grok_access_token, grok_oauth_base_url, oauth_credential_count, responses_endpoint,
    select_codex_auth, select_oauth_credential,
};
use serde_json::json;

#[test]
fn responses_endpoint_appends_responses_to_versioned_base_path() {
    let endpoint = responses_endpoint("https://api.example.com/codex/v1").expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/codex/v1/responses");
}

#[test]
fn responses_endpoint_keeps_explicit_responses_path() {
    let endpoint = responses_endpoint("https://api.example.com/v1/responses").expect("endpoint");
    assert_eq!(endpoint, "https://api.example.com/v1/responses");
}

#[test]
fn codex_oauth_base_url_prefers_auth_base_url() {
    let auth: CodexAuth = serde_json::from_value(json!({
        "base_url": "https://chatgpt.example/backend-api/codex/"
    }))
    .unwrap();

    let base_url = codex_oauth_base_url(Some("https://provider.example/codex"), &auth);

    assert_eq!(base_url, "https://chatgpt.example/backend-api/codex");
}

#[test]
fn grok_oauth_base_url_prefers_auth_base_url() {
    let auth: GrokAuth = serde_json::from_value(json!({
        "base_url": "https://cli-chat-proxy.grok.com/v1/"
    }))
    .unwrap();

    let base_url = grok_oauth_base_url(Some("https://api.x.ai/v1"), &auth);

    assert_eq!(base_url, "https://cli-chat-proxy.grok.com/v1");
}

#[test]
fn codex_auth_selection_starts_from_success_cursor() {
    let auth: OneOrMany<CodexAuth> = serde_json::from_value(json!([
        { "access_token": "first" },
        { "access_token": "disabled", "disabled": true },
        { "access_token": "second" }
    ]))
    .unwrap();

    let (idx, selected) = select_codex_auth(&auth, None, 1).unwrap();
    assert_eq!(idx, 0);
    assert_eq!(selected.access_token.as_deref(), Some("first"));

    let (idx, selected) = select_codex_auth(&auth, None, 2).unwrap();
    assert_eq!(idx, 2);
    assert_eq!(selected.access_token.as_deref(), Some("second"));
}

#[tokio::test]
async fn codex_access_token_uses_unexpired_access_token_without_refresh_token() {
    let client = reqwest::Client::new();
    let mut auth: CodexAuth = serde_json::from_value(json!({
        "access_token": "codex-access-token"
    }))
    .unwrap();

    let token = codex_access_token(&client, &mut auth).await.unwrap();

    assert_eq!(token.token, "codex-access-token");
    assert!(!token.refreshed);
}

#[tokio::test]
async fn grok_access_token_uses_access_token_without_expiry_check() {
    let client = reqwest::Client::new();
    let mut auth: GrokAuth = serde_json::from_value(json!({
        "access_token": "expired-grok-token",
        "expiry_date": current_millis() - 1_000
    }))
    .unwrap();

    let token = grok_access_token(&client, &mut auth).await.unwrap();

    assert_eq!(token.token, "expired-grok-token");
    assert!(!token.refreshed);
}

#[test]
fn oauth_credential_count_includes_api_key_and_enabled_auths() {
    let auth: OneOrMany<CodexAuth> = serde_json::from_value(json!([
        { "access_token": "first" },
        { "access_token": "disabled", "disabled": true },
        { "access_token": "second" }
    ]))
    .unwrap();

    assert_eq!(oauth_credential_count("", &auth), 2);
    assert_eq!(oauth_credential_count("sk-test", &auth), 3);
    assert_eq!(
        oauth_credential_count("sk-test", &OneOrMany::<CodexAuth>::Many(vec![])),
        1
    );
}

#[test]
fn select_oauth_credential_prefers_api_key_then_rotates_auth() {
    let auth: OneOrMany<CodexAuth> = serde_json::from_value(json!([
        { "access_token": "first" },
        { "access_token": "second" }
    ]))
    .unwrap();

    match select_oauth_credential("sk-api", &auth, None, 1, "Codex").unwrap() {
        SelectedCredential::ApiKey { token } => assert_eq!(token, "sk-api"),
        other => panic!("expected api key, got {other:?}"),
    }
    match select_oauth_credential("sk-api", &auth, None, 2, "Codex").unwrap() {
        SelectedCredential::Auth { index, auth } => {
            assert_eq!(index, 0);
            assert_eq!(auth.access_token.as_deref(), Some("first"));
        }
        other => panic!("expected auth 0, got {other:?}"),
    }
    match select_oauth_credential("sk-api", &auth, None, 3, "Codex").unwrap() {
        SelectedCredential::Auth { index, auth } => {
            assert_eq!(index, 1);
            assert_eq!(auth.access_token.as_deref(), Some("second"));
        }
        other => panic!("expected auth 1, got {other:?}"),
    }
    // Start from last successful auth index and continue rotating (including api_key wrap).
    match select_oauth_credential("sk-api", &auth, Some(1), 1, "Codex").unwrap() {
        SelectedCredential::Auth { index, .. } => assert_eq!(index, 1),
        other => panic!("expected auth 1 start, got {other:?}"),
    }
    match select_oauth_credential("sk-api", &auth, Some(1), 2, "Codex").unwrap() {
        SelectedCredential::ApiKey { token } => assert_eq!(token, "sk-api"),
        other => panic!("expected wrap to api key, got {other:?}"),
    }
}
