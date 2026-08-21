use llm_proxy::middleware::headers::{filter_response_headers, get_forwardable_request_headers};
use llm_proxy::provider::types::HeaderMap;
use salvo::http::{HeaderMap as ReqHeaderMap, HeaderName, HeaderValue};

fn req_headers(pairs: &[(&str, &str)]) -> ReqHeaderMap {
    let mut headers = ReqHeaderMap::new();
    for (name, value) in pairs {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    headers
}

#[test]
fn get_forwardable_request_headers_strips_accept_encoding() {
    // Clients commonly send `accept-encoding: gzip, br`; forwarding it would make
    // upstreams return a compressed body while the proxy strips `content-encoding`,
    // producing binary output for non-streaming responses.
    let headers = req_headers(&[
        ("accept-encoding", "gzip, br, deflate"),
        ("authorization", "Bearer secret"),
        ("content-length", "42"),
        ("cookie", "session=abc"),
        ("user-agent", "llm-proxy-test"),
        ("x-request-id", "1234"),
        ("x-goog-api-key", "secret"),
    ]);

    let forwarded = get_forwardable_request_headers(&headers);

    assert!(!forwarded.contains_key("accept-encoding"));
    assert!(!forwarded.contains_key("authorization"));
    assert!(!forwarded.contains_key("content-length"));
    assert!(!forwarded.contains_key("cookie"));
    assert!(!forwarded.contains_key("x-goog-api-key"));

    assert_eq!(forwarded.get("user-agent").map(|v| v.as_str()), Some("llm-proxy-test"));
    assert_eq!(forwarded.get("x-request-id").map(|v| v.as_str()), Some("1234"));
}

#[test]
fn filter_response_headers_strips_encoding_and_length() {
    let headers: HeaderMap = [
        ("content-type", "application/json"),
        ("content-encoding", "gzip"),
        ("content-length", "123"),
        ("x-request-id", "4567"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();

    let filtered = filter_response_headers(&headers);

    assert!(!filtered.contains_key("content-encoding"));
    assert!(!filtered.contains_key("content-length"));
    assert_eq!(
        filtered.get("content-type").map(|v| v.as_str()),
        Some("application/json")
    );
    assert_eq!(filtered.get("x-request-id").map(|v| v.as_str()), Some("4567"));
}
