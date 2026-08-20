use llm_proxy::provider::types::ProviderType;
use llm_proxy::provider::{Providers, has_rewrite, rewrite_request, wire_protocol};
use serde_json::{Value, json};

fn tool_type(tool: &Value) -> Option<&str> {
    tool.get("type").and_then(Value::as_str)
}

#[test]
fn profiles_expose_wire_and_rewrite_flags() {
    // Grok is now a first-class wire protocol; Codex still speaks Responses.
    assert_eq!(wire_protocol(ProviderType::Grok), ProviderType::Grok);
    assert_eq!(wire_protocol(ProviderType::Codex), ProviderType::Responses);
    assert_eq!(wire_protocol(ProviderType::Gemini), ProviderType::Gemini);
    assert_eq!(wire_protocol(ProviderType::Chat), ProviderType::Chat);
    assert!(has_rewrite(ProviderType::Grok));
    assert!(has_rewrite(ProviderType::Codex));
    assert!(!has_rewrite(ProviderType::Gemini));
    assert!(!has_rewrite(ProviderType::Responses));
    assert!(!has_rewrite(ProviderType::Chat));
    assert!(!has_rewrite(ProviderType::Claude));
}

#[test]
fn skip_convert_and_rewrite_when_same_wire_and_no_dialect() {
    let providers = Providers::new();
    let body = json!({
        "model": "gemini-2.5-pro",
        "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
        "keep_me": true
    });

    let out = providers
        .prepare_request(
            ProviderType::Gemini,
            body.clone(),
            ProviderType::Gemini,
            true,
        )
        .expect("prepare");

    // No convert, no rewrite, Gemini does not force stream.
    assert_eq!(out, body);
    assert!(out.get("stream").is_none());
}

#[test]
fn responses_passthrough_skips_convert_but_sets_stream() {
    let providers = Providers::new();
    let body = json!({
        "model": "gpt",
        "input": [{"role": "user", "content": "hi"}],
        "tools": [{"type": "web_search", "external_web_access": false}]
    });

    let out = providers
        .prepare_request(
            ProviderType::Responses,
            body.clone(),
            ProviderType::Responses,
            true,
        )
        .expect("prepare");

    assert_eq!(out["tools"][0]["external_web_access"], false);
    assert_eq!(out["stream"], true);
    assert_eq!(out["input"], body["input"]);
}

#[test]
fn grok_rewrite_only_when_source_already_grok() {
    let providers = Providers::new();
    // Client already speaks the Grok wire protocol (source == wire == Grok):
    // dialect rewrite fires and injects a bare x_search alongside client tools.
    let body = json!({
        "model": "grok-4.5",
        "input": [{"role": "user", "content": "hi"}],
        "tools": [
            {
                "type": "function",
                "name": "read_file",
                "parameters": {"type": "object", "properties": {}}
            }
        ]
    });

    let out = providers
        .prepare_request(ProviderType::Grok, body, ProviderType::Grok, true)
        .expect("prepare_request");

    assert_eq!(out["stream"], true);
    let tools = out.get("tools").and_then(Value::as_array).expect("tools");
    let types: Vec<&str> = tools.iter().filter_map(tool_type).collect();
    assert_eq!(types, vec!["function", "x_search"]);
}

#[test]
fn grok_skips_dialect_rewrite_when_endpoint_is_not_wire() {
    let providers = Providers::new();
    // Chat endpoint protocol != Grok wire (Grok) → convert only, no rewrite.
    let body = json!({
        "model": "grok-4.5",
        "messages": [{"role": "user", "content": "hi"}],
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "read_file",
                    "parameters": {"type": "object", "properties": {}}
                }
            }
        ]
    });

    let out = providers
        .prepare_request(ProviderType::Grok, body, ProviderType::Chat, true)
        .expect("prepare_request");

    assert_eq!(out["stream"], true);
    // Cross-protocol entry converts to the Grok wire shape but skips dialect
    // rewrite, so no bare x_search is injected.
    if let Some(tools) = out.get("tools").and_then(Value::as_array) {
        assert!(
            !tools.iter().any(|tool| tool_type(tool) == Some("x_search")),
            "cross-protocol entry must skip grok dialect rewrite: {tools:?}"
        );
    }
}

#[test]
fn gemini_endpoint_to_grok_converts_but_skips_rewrite() {
    let providers = Providers::new();
    let body = json!({
        "model": "gemini-2.5-pro",
        "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
        "tools": [{
            "functionDeclarations": [{
                "name": "read_file",
                "description": "read a file",
                "parameters": {"type": "object", "properties": {}}
            }]
        }]
    });

    let out = providers
        .prepare_request(ProviderType::Grok, body, ProviderType::Gemini, true)
        .expect("prepare_request");

    assert_eq!(out["stream"], true);
    // Converted toward the Grok wire shape; rewrite is skipped on cross-protocol
    // entry, so x_search is not injected.
    if let Some(tools) = out.get("tools").and_then(Value::as_array) {
        assert!(
            !tools.iter().any(|tool| tool_type(tool) == Some("x_search")),
            "gemini endpoint + grok must skip dialect rewrite: {tools:?}"
        );
    }
}

#[test]
fn codex_skips_dialect_rewrite_when_endpoint_is_not_wire() {
    let providers = Providers::new();
    let body = json!({
        "model": "codex",
        "messages": [{"role": "user", "content": "hi"}],
        "temperature": 0.5
    });

    let out = providers
        .prepare_request(ProviderType::Codex, body, ProviderType::Chat, false)
        .expect("prepare_request");

    assert_eq!(out["stream"], false);
    // Codex rewrite would strip temperature and force store=false.
    assert_eq!(out.get("temperature"), Some(&json!(0.5)));
    assert!(out.get("store").is_none());
}

#[test]
fn codex_dialect_removes_fields_and_sets_store() {
    let body = json!({
        "model": "codex",
        "temperature": 0.2,
        "max_output_tokens": 128,
        "input": []
    });
    let out = rewrite_request(ProviderType::Codex, body).unwrap();
    assert!(out.get("temperature").is_none());
    assert!(out.get("max_output_tokens").is_none());
    assert_eq!(out["store"], false);
}

#[test]
fn codex_prepare_request_uses_provider_profile() {
    let providers = Providers::new();
    let body = json!({
        "model": "codex",
        "temperature": 0.5,
        "max_output_tokens": 256,
        "input": [{"role": "user", "content": "hi"}],
        "tools": [
            {
                "type": "function",
                "name": "echo",
                "parameters": {"type": "object"}
            }
        ]
    });

    let out = providers
        .prepare_request(ProviderType::Codex, body, ProviderType::Responses, false)
        .expect("prepare_request");

    assert!(out.get("temperature").is_none());
    assert!(out.get("max_output_tokens").is_none());
    assert_eq!(out["store"], false);
    assert_eq!(out["stream"], false);
    assert_eq!(out["tools"][0]["type"], "function");
}

#[test]
fn grok_profile_injects_x_search_when_tools_present() {
    let body = json!({
        "model": "grok-4.5",
        "input": [{"role": "user", "content": "hi"}],
        "tools": [
            {"type": "function", "name": "echo", "parameters": {"type": "object"}}
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let tools = out.get("tools").and_then(Value::as_array).unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["name"], "echo");
    assert_eq!(tools[1], json!({"type": "x_search"}));
}

#[test]
fn grok_profile_does_not_create_tools_only_for_x_search() {
    let body = json!({
        "model": "grok-4.5",
        "input": [{"role": "user", "content": "hi"}]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    assert!(out.get("tools").is_none());
}

#[test]
fn grok_profile_does_not_duplicate_existing_x_search() {
    let body = json!({
        "tools": [
            {
                "type": "x_search",
                "allowed_x_handles": ["elonmusk"]
            }
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let tools = out.get("tools").and_then(Value::as_array).unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["type"], "x_search");
    assert_eq!(tools[0]["allowed_x_handles"], json!(["elonmusk"]));
}

#[test]
fn passthrough_rewrite_is_identity() {
    let body = json!({"model": "x", "tools": [{"type": "function", "name": "a"}]});
    let out = rewrite_request(ProviderType::Responses, body.clone()).unwrap();
    assert_eq!(out, body);
}

#[test]
fn chat_passthrough_normalizes_developer_role_to_system() {
    let providers = Providers::new();
    let body = json!({
        "model": "glm-5.2",
        "messages": [
            {"role": "developer", "content": "you are helpful"},
            {"role": "user", "content": "hi"},
            {"role": "system", "content": "keep system"},
            {"role": "assistant", "content": "ok"}
        ]
    });

    let out = providers
        .prepare_request(ProviderType::Chat, body, ProviderType::Chat, true)
        .expect("prepare_request");

    let roles: Vec<&str> = out["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|m| m["role"].as_str().expect("role"))
        .collect();
    // developer folds into system; other roles are untouched.
    assert_eq!(roles, vec!["system", "user", "system", "assistant"]);
    assert_eq!(out["messages"][0]["content"], "you are helpful");
    assert_eq!(out["stream"], true);
}

#[test]
fn chat_normalize_developer_role_after_cross_protocol_convert() {
    let providers = Providers::new();
    // Responses entry with a developer message converts to the Chat wire
    // (which preserves developer), then the always-on compat step must fold
    // developer into system so compatible backends accept the request.
    let body = json!({
        "model": "glm-5.2",
        "input": [
            {"role": "developer", "content": "you are helpful"},
            {"role": "user", "content": "hi"}
        ]
    });

    let out = providers
        .prepare_request(ProviderType::Chat, body, ProviderType::Responses, true)
        .expect("prepare_request");

    let roles: Vec<&str> = out["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|m| m["role"].as_str().expect("role"))
        .collect();
    assert!(
        !roles.contains(&"developer"),
        "developer role must be normalized to system on the chat wire: {roles:?}"
    );
    assert!(roles.contains(&"system"));
    assert_eq!(out["stream"], true);
}

#[test]
fn chat_normalize_is_safe_without_messages_array() {
    let providers = Providers::new();
    let body = json!({"model": "glm-5.2", "prompt": "hi"});
    let out = providers
        .prepare_request(ProviderType::Chat, body, ProviderType::Chat, false)
        .expect("prepare_request");
    assert_eq!(out["prompt"], "hi");
    assert_eq!(out["stream"], false);
}
