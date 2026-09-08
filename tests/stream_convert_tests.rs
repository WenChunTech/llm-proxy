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
    assert!(
        data["type"].is_string(),
        "converted event must carry a type"
    );
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
    assert!(
        data["type"].is_string(),
        "converted event must carry a type"
    );
}

#[test]
fn responses_to_claude_interleaved_function_calls() {
    // Regression: when OpenAI Responses interleaves multiple function calls
    // (output_item.added for indices 1,2,3 before any output_item.done),
    // the converter must buffer function calls and flush them as well-formed
    // Claude content blocks (start → delta → stop) with unique indices.
    let mut converter = StreamConverterImpl::new(
        ProviderType::Responses,
        ProviderType::Claude,
        StreamContext::default(),
    );

    let sse = |event: &str, data: Value| SseEvent {
        event: Some(event.to_string()),
        data: data.to_string(),
        id: None,
        retry: None,
    };

    // 1. response.created → message_start
    let events = converter.convert_event(sse("response.created", json!({
        "type": "response.created",
        "sequence_number": 1,
        "response": {"id": "resp_1", "object": "response", "created_at": 1, "status": "in_progress", "model": "test-model", "output": []}
    }))).unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.event.as_deref() == Some("message_start"))
    );

    // 2. output_item.added (text, oi=0) → content_block_start
    converter.convert_event(sse("response.output_item.added", json!({
        "type": "response.output_item.added", "sequence_number": 3, "output_index": 0,
        "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "in_progress"}
    }))).unwrap();

    // 3. text delta + output_item.done (oi=0)
    converter
        .convert_event(sse(
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta", "sequence_number": 4, "output_index": 0,
                "item_id": "msg_1", "content_index": 0, "delta": "hello"
            }),
        ))
        .unwrap();
    converter.convert_event(sse("response.output_item.done", json!({
        "type": "response.output_item.done", "sequence_number": 5, "output_index": 0,
        "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "completed", "content": [{"type": "output_text", "text": "hello", "annotations": [], "logprobes": []}]}
    }))).unwrap();

    // 4. output_item.added (function_call, oi=1) → should BUFFER (no content_block_start yet)
    let ev_add1 = converter.convert_event(sse("response.output_item.added", json!({
        "type": "response.output_item.added", "sequence_number": 6, "output_index": 1,
        "item": {"type": "function_call", "id": "fc_call_1", "call_id": "call_1", "name": "Read", "arguments": "", "status": "in_progress"}
    }))).unwrap();
    assert!(
        ev_add1.is_empty(),
        "output_item.added for function_call should be buffered (no events), got: {:?}",
        ev_add1
            .iter()
            .map(|e| e.event.as_deref())
            .collect::<Vec<_>>()
    );

    // 5. function_call_arguments.delta (oi=1) → should BUFFER (no content_block_delta yet)
    let ev_args1 = converter.convert_event(sse("response.function_call_arguments.delta", json!({
        "type": "response.function_call_arguments.delta", "sequence_number": 7, "output_index": 1, "item_id": "fc_call_1", "delta": "{\"a\":1}"
    }))).unwrap();
    assert!(ev_args1.is_empty(), "arguments.delta should be buffered");

    // 6. output_item.added (function_call, oi=2) — interleaved! → should BUFFER
    let ev_add2 = converter.convert_event(sse("response.output_item.added", json!({
        "type": "response.output_item.added", "sequence_number": 8, "output_index": 2,
        "item": {"type": "function_call", "id": "fc_call_2", "call_id": "call_2", "name": "Read", "arguments": "", "status": "in_progress"}
    }))).unwrap();
    assert!(
        ev_add2.is_empty(),
        "second output_item.added should also be buffered"
    );

    // 7. function_call_arguments.delta (oi=2) → should BUFFER
    let ev_args2 = converter.convert_event(sse("response.function_call_arguments.delta", json!({
        "type": "response.function_call_arguments.delta", "sequence_number": 9, "output_index": 2, "item_id": "fc_call_2", "delta": "{\"b\":2}"
    }))).unwrap();
    assert!(
        ev_args2.is_empty(),
        "second arguments.delta should be buffered"
    );

    // 8. output_item.done (oi=1) → flush: start + delta + stop
    let flush1 = converter.convert_event(sse("response.output_item.done", json!({
        "type": "response.output_item.done", "sequence_number": 10, "output_index": 1,
        "item": {"type": "function_call", "id": "fc_call_1", "call_id": "call_1", "name": "Read", "arguments": "{\"a\":1}", "status": "completed"}
    }))).unwrap();

    // Must produce exactly: content_block_start, content_block_delta, content_block_stop
    let types: Vec<_> = flush1
        .iter()
        .map(|e| e.event.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(
        types,
        vec![
            "content_block_start",
            "content_block_delta",
            "content_block_stop"
        ],
        "flush must produce start → delta → stop, got: {:?}",
        types
    );

    // 9. output_item.done (oi=2) → flush: start + delta + stop (with different index)
    let flush2 = converter.convert_event(sse("response.output_item.done", json!({
        "type": "response.output_item.done", "sequence_number": 11, "output_index": 2,
        "item": {"type": "function_call", "id": "fc_call_2", "call_id": "call_2", "name": "Read", "arguments": "{\"b\":2}", "status": "completed"}
    }))).unwrap();

    let types2: Vec<_> = flush2
        .iter()
        .map(|e| e.event.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(
        types2,
        vec![
            "content_block_start",
            "content_block_delta",
            "content_block_stop"
        ],
        "second flush must also produce start → delta → stop"
    );

    // Verify: the two content_block_stop events have DIFFERENT indices
    let stop_idx = |events: &[llm_proxy::stream::sse::OutboundSseEvent]| -> Option<u32> {
        for e in events {
            if e.event.as_deref() == Some("content_block_stop") {
                let v: Value = serde_json::from_str(&e.data).unwrap();
                return v.get("index").and_then(|i| i.as_u64()).map(|i| i as u32);
            }
        }
        None
    };

    let idx1 = stop_idx(&flush1);
    let idx2 = stop_idx(&flush2);
    assert_ne!(
        idx1, idx2,
        "interleaved function calls must have distinct content block indices"
    );
}
