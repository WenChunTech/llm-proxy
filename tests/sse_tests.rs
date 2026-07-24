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
