use llm_proxy::{
    provider::types::ProviderType,
    stream::{
        convert::{StreamContext, StreamConverterImpl},
        sse::SseEvent,
    },
};
use serde_json::{Value, json};

#[test]
fn grok_stream_output_item_added_without_content_converts_to_responses() {
    let mut converter = StreamConverterImpl::new(
        ProviderType::Grok,
        ProviderType::Responses,
        StreamContext::default(),
    );
    let event = SseEvent {
        event: Some("response.output_item.added".to_string()),
        data: json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "sequence_number": 1,
            "item": {
                "type": "message",
                "id": "msg_1",
                "role": "assistant",
                "status": "in_progress"
            }
        })
        .to_string(),
        id: None,
        retry: None,
    };

    let events = converter
        .convert_event(event)
        .expect("Grok stream event should convert to Responses");

    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].event.as_deref(),
        Some("response.output_item.added")
    );

    let data: Value = serde_json::from_str(&events[0].data).expect("converted SSE data is JSON");
    assert_eq!(data["item"]["content"], json!([]));
}

#[test]
fn responses_stream_text_delta_converts_to_grok() {
    // Regression: convert_responses with target=Grok used to return Ok(Vec::new())
    // for every chunk, producing an empty client response for /grok/v1/responses
    // when the upstream was an OpenAI Responses provider.
    let mut converter = StreamConverterImpl::new(
        ProviderType::Responses,
        ProviderType::Grok,
        StreamContext::default(),
    );
    let event = SseEvent {
        event: Some("response.output_text.delta".to_string()),
        data: json!({
            "type": "response.output_text.delta",
            "content_index": 0,
            "delta": "Hi",
            "item_id": "msg_1",
            "logprobs": [],
            "output_index": 0,
            "sequence_number": 3,
        })
        .to_string(),
        id: None,
        retry: None,
    };

    let events = converter
        .convert_event(event)
        .expect("Responses stream event should convert to Grok");

    assert!(!events.is_empty(), "Grok conversion must not be empty");
    let data: Value = serde_json::from_str(&events[0].data).expect("Grok SSE data is JSON");
    assert_eq!(data["type"], "response.output_text.delta");
    assert_eq!(data["delta"], "Hi");
    assert_eq!(data["item_id"], "msg_1");
}

#[test]
fn claude_stream_message_start_converts_to_grok() {
    // Regression: convert_claude with target=Grok used to return Ok(Vec::new()).
    let mut converter = StreamConverterImpl::new(
        ProviderType::Claude,
        ProviderType::Grok,
        StreamContext::default(),
    );
    let event = SseEvent {
        event: Some("message_start".to_string()),
        data: json!({
            "type": "message_start",
            "message": {
                "id": "msg_1",
                "type": "message",
                "role": "assistant",
                "model": "claude-3",
                "stop_reason": null,
                "stop_sequence": null,
                "usage": { "input_tokens": 10, "output_tokens": 1 },
            },
        })
        .to_string(),
        id: None,
        retry: None,
    };

    let events = converter
        .convert_event(event)
        .expect("Claude stream event should convert to Grok");

    assert!(!events.is_empty(), "Grok conversion must not be empty");
    let data: Value = serde_json::from_str(&events[0].data).expect("Grok SSE data is JSON");
    assert!(data["type"].is_string(), "converted event must carry a type");
}

#[test]
fn gemini_stream_text_candidate_converts_to_grok() {
    // Regression: convert_gemini with target=Grok used to return Ok(Vec::new()).
    let mut converter = StreamConverterImpl::new(
        ProviderType::Gemini,
        ProviderType::Grok,
        StreamContext::default(),
    );
    let event = SseEvent {
        event: Some("generate_content".to_string()),
        data: json!({
            "modelVersion": "gemini-1.5",
            "candidates": [
                {
                    "content": {
                        "role": "model",
                        "parts": [{ "text": "Hi" }]
                    }
                }
            ],
        })
        .to_string(),
        id: None,
        retry: None,
    };

    let events = converter
        .convert_event(event)
        .expect("Gemini stream event should convert to Grok");

    assert!(!events.is_empty(), "Grok conversion must not be empty");
    let data: Value = serde_json::from_str(&events[0].data).expect("Grok SSE data is JSON");
    assert!(data["type"].is_string(), "converted event must carry a type");
}
