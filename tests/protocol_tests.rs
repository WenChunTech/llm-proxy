use llm_proxy::protocol::{SUPPORTED_PROTOCOL_CONVERSIONS, convert_request, convert_response};
use llm_proxy::provider::types::ProviderType;
use serde_json::{Value, json};

fn assert_object(value: &Value) {
    assert!(value.is_object(), "converted value must be an object: {value}");
}

#[test]
fn protocol_conversion_matrix_covers_all_non_identity_core_pairs() {
    // Grok is a first-class protocol with its own native converters, so it
    // participates in the conversion matrix alongside the core protocols.
    let protocols = [
        ProviderType::Chat,
        ProviderType::Responses,
        ProviderType::Claude,
        ProviderType::Gemini,
        ProviderType::Grok,
    ];

    for source in protocols {
        for target in protocols {
            if source == target {
                continue;
            }
            assert!(
                SUPPORTED_PROTOCOL_CONVERSIONS.contains(&(source, target)),
                "missing conversion matrix entry: {source:?} -> {target:?}"
            );
        }
    }
}

#[test]
fn protocol_conversion_matrix_excludes_codex_alias() {
    // Codex speaks the OpenAI Responses wire protocol, so it must never appear
    // in the conversion matrix. Grok, by contrast, is first-class.
    assert!(
        !SUPPORTED_PROTOCOL_CONVERSIONS
            .iter()
            .any(|(source, target)| matches!(source, ProviderType::Codex)
                || matches!(target, ProviderType::Codex))
    );
}

#[test]
fn grok_request_conversions_from_core_protocols() {
    // Chat -> Grok
    let chat = json!({
        "model": "grok-4.5",
        "messages": [{"role": "user", "content": "hi"}]
    });
    let out = convert_request(chat, ProviderType::Chat, ProviderType::Grok).expect("chat->grok");
    assert_object(&out);
    assert_eq!(out["model"], "grok-4.5");

    // Responses -> Grok
    let responses = json!({"model": "grok-4.5", "input": "hi"});
    let out =
        convert_request(responses, ProviderType::Responses, ProviderType::Grok).expect("resp->grok");
    assert_object(&out);

    // Claude -> Grok
    let claude = json!({
        "model": "grok-4.5",
        "max_tokens": 64,
        "messages": [{"role": "user", "content": "hi"}]
    });
    let out = convert_request(claude, ProviderType::Claude, ProviderType::Grok).expect("claude->grok");
    assert_object(&out);

    // Gemini -> Grok (routed through gemini_cli)
    let gemini = json!({
        "model": "grok-4.5",
        "contents": [{"role": "user", "parts": [{"text": "hi"}]}]
    });
    let out = convert_request(gemini, ProviderType::Gemini, ProviderType::Grok).expect("gemini->grok");
    assert_object(&out);
}

#[test]
fn grok_request_conversions_to_core_protocols() {
    let grok = json!({
        "model": "grok-4.5",
        "input": "hi",
        "temperature": 0.3
    });

    let chat = convert_request(grok.clone(), ProviderType::Grok, ProviderType::Chat).expect("grok->chat");
    assert_eq!(chat["model"], "grok-4.5");
    assert!(chat.get("messages").is_some());

    let responses =
        convert_request(grok.clone(), ProviderType::Grok, ProviderType::Responses).expect("grok->resp");
    assert_object(&responses);

    let claude = convert_request(grok.clone(), ProviderType::Grok, ProviderType::Claude).expect("grok->claude");
    assert_eq!(claude["model"], "grok-4.5");

    let gemini = convert_request(grok, ProviderType::Grok, ProviderType::Gemini).expect("grok->gemini");
    assert_object(&gemini);
}

#[test]
fn grok_response_conversions_round_trip() {
    let grok = json!({
        "id": "resp_1",
        "object": "response",
        "created_at": 1,
        "model": "grok-4.5",
        "status": "completed",
        "output": [{
            "type": "message",
            "role": "assistant",
            "status": "completed",
            "content": [{"type": "output_text", "text": "world"}]
        }]
    });

    let chat = convert_response(grok.clone(), ProviderType::Grok, ProviderType::Chat).expect("grok->chat");
    assert_eq!(chat["model"], "grok-4.5");
    assert!(chat.get("choices").is_some());

    let responses =
        convert_response(grok.clone(), ProviderType::Grok, ProviderType::Responses).expect("grok->resp");
    assert_object(&responses);

    let claude = convert_response(grok.clone(), ProviderType::Grok, ProviderType::Claude).expect("grok->claude");
    assert_object(&claude);

    let gemini = convert_response(grok, ProviderType::Grok, ProviderType::Gemini).expect("grok->gemini");
    assert_object(&gemini);
}

#[test]
fn grok_identity_conversion_is_passthrough() {
    let body = json!({"model": "grok-4.5", "input": "hi"});
    let out = convert_request(body.clone(), ProviderType::Grok, ProviderType::Grok).expect("identity");
    assert_eq!(out, body);
}
