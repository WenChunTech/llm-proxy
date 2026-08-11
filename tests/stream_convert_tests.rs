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
