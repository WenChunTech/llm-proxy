use llm_proxy::provider::types::ProviderType;
use llm_proxy::provider::{
    Providers, has_response_rewrite, prepare_response, response_wire_protocol, rewrite_response,
};
use serde_json::{Value, json};

#[test]
fn profiles_expose_wire_and_rewrite_flags() {
    assert_eq!(
        response_wire_protocol(ProviderType::Chat),
        ProviderType::Chat
    );
    assert_eq!(
        response_wire_protocol(ProviderType::Codex),
        ProviderType::Responses
    );
    assert_eq!(
        response_wire_protocol(ProviderType::Grok),
        ProviderType::Grok
    );
    assert_eq!(
        response_wire_protocol(ProviderType::Claude),
        ProviderType::Claude
    );

    assert!(has_response_rewrite(ProviderType::Chat));
    assert!(!has_response_rewrite(ProviderType::Responses));
    assert!(!has_response_rewrite(ProviderType::Claude));
    assert!(!has_response_rewrite(ProviderType::Gemini));
    assert!(!has_response_rewrite(ProviderType::Codex));
    assert!(!has_response_rewrite(ProviderType::Grok));
}

#[test]
fn empty_finish_reason_becomes_null() {
    let body = json!({
        "id": "1",
        "choices": [{
            "index": 0,
            "delta": {"content": "hi"},
            "finish_reason": ""
        }]
    });

    let rewritten = rewrite_response(ProviderType::Chat, body).unwrap();
    assert_eq!(rewritten["choices"][0]["finish_reason"], Value::Null);
}

#[test]
fn real_finish_reason_is_preserved() {
    let body = json!({
        "choices": [{
            "index": 0,
            "finish_reason": "stop"
        }]
    });

    let rewritten = rewrite_response(ProviderType::Chat, body).unwrap();
    assert_eq!(
        rewritten["choices"][0]["finish_reason"],
        Value::String("stop".to_string())
    );
}

#[test]
fn non_chat_source_is_passthrough() {
    let body = json!({"choices":[{"finish_reason":""}]});
    let rewritten = rewrite_response(ProviderType::Claude, body.clone()).unwrap();
    assert_eq!(rewritten, body);
}

#[test]
fn prepare_response_rewrites_then_converts_chat_to_claude() {
    let body = json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "created": 1,
        "model": "glm-5.2",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "hi",
                "refusal": ""
            },
            "finish_reason": ""
        }]
    });

    let converted = prepare_response(ProviderType::Chat, body, ProviderType::Claude)
        .expect("empty finish_reason should be rewritten before conversion");

    assert_eq!(
        converted.get("type").and_then(Value::as_str),
        Some("message")
    );
}

#[test]
fn convert_response_uses_prepare_pipeline() {
    let providers = Providers::new();
    let body = json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "created": 1,
        "model": "glm-5.2",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "hi",
                "refusal": ""
            },
            "finish_reason": ""
        }]
    });

    let converted = providers
        .convert_response(ProviderType::Chat, body, ProviderType::Claude)
        .expect("Providers::convert_response should use response rewrite pipeline");

    assert_eq!(
        converted.get("type").and_then(Value::as_str),
        Some("message")
    );
}
