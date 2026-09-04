use llm_proxy::stream::sse::SseParser;

#[test]
fn parses_multiline_sse_data() {
    let mut parser = SseParser::default();
    let events = parser
        .push(b"event: message\ndata: a\ndata: b\n\n")
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event.as_deref(), Some("message"));
    assert_eq!(events[0].data, "a\nb");
}

#[test]
fn buffers_partial_lines() {
    let mut parser = SseParser::default();
    assert!(parser.push(b"data: {\"a\"").unwrap().is_empty());
    let events = parser.push(b":1}\n\n").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, "{\"a\":1}");
}

#[test]
fn parses_multibyte_utf8_split_across_chunks() {
    let payload = "data: > 范围：审计\n\n";
    let bytes = payload.as_bytes();
    // '范' is e8 8c 83; cut after two of its bytes, like a TCP chunk boundary.
    let split = payload.find('范').unwrap() + 2;

    let mut parser = SseParser::default();
    assert!(parser.push(&bytes[..split]).unwrap().is_empty());
    let events = parser.push(&bytes[split..]).unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, "> 范围：审计");
}

#[test]
fn parses_stream_fed_one_byte_at_a_time() {
    let payload = "event: response.output_item.added\ndata: {\"text\":\"你好，世界 🎉\"}\n\n";

    let mut parser = SseParser::default();
    let mut events = Vec::new();
    for byte in payload.as_bytes() {
        events.extend(parser.push(&[*byte]).unwrap());
    }
    events.extend(parser.finish().unwrap());

    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].event.as_deref(),
        Some("response.output_item.added")
    );
    assert_eq!(events[0].data, "{\"text\":\"你好，世界 🎉\"}");
}

#[test]
fn handles_crlf_with_split_multibyte() {
    let payload = "data: 数据\r\n\r\n";
    let bytes = payload.as_bytes();
    // '数' is e6 95 b0; split inside it.
    let split = payload.find('数').unwrap() + 1;

    let mut parser = SseParser::default();
    assert!(parser.push(&bytes[..split]).unwrap().is_empty());
    let events = parser.push(&bytes[split..]).unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, "数据");
}

#[test]
fn rejects_invalid_utf8_in_complete_line() {
    let mut parser = SseParser::default();
    let error = parser.push(b"data: \xf0\x28\x8c\x28\n\n").unwrap_err();
    assert!(error.to_string().contains("invalid utf-8"));
}

#[test]
fn finish_flushes_tail_without_trailing_newline() {
    let mut parser = SseParser::default();
    assert!(parser.push("data: 你好".as_bytes()).unwrap().is_empty());
    let event = parser.finish().unwrap().expect("tail without newline");
    assert_eq!(event.data, "你好");
    assert!(parser.finish().unwrap().is_none());
}

#[test]
fn finish_decodes_tail_ending_mid_character() {
    let payload = "data: 你".as_bytes().to_vec();
    // '你' is e4 bd a0; withhold the final continuation byte.
    let truncated = &payload[..payload.len() - 1];

    let mut parser = SseParser::default();
    assert!(parser.push(truncated).unwrap().is_empty());
    let event = parser.finish().unwrap().expect("truncated tail flushes");
    assert_eq!(event.data, "\u{fffd}");
    assert!(parser.finish().unwrap().is_none());
}
