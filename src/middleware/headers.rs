use std::collections::HashMap;

use salvo::http::HeaderMap;

const REQUEST_BLOCKLIST: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "authorization",
    "content-length",
    "cookie",
    "host",
    "proxy-connection",
    "x-api-key",
    "x-goog-api-key",
    "accept-encoding",
];

const RESPONSE_BLOCKLIST: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "content-encoding",
    "content-length",
];

pub fn get_forwardable_request_headers(headers: &HeaderMap) -> crate::provider::types::HeaderMap {
    headers
        .iter()
        .filter_map(|(name, value)| {
            let name = name.as_str().to_ascii_lowercase();
            if header_blocked(&name, REQUEST_BLOCKLIST) {
                return None;
            }
            value
                .to_str()
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(|value| (name, value.to_string()))
        })
        .collect()
}

pub fn filter_response_headers(
    headers: &crate::provider::types::HeaderMap,
) -> crate::provider::types::HeaderMap {
    headers
        .iter()
        .filter(|(name, value)| {
            !header_blocked(name, RESPONSE_BLOCKLIST) && !value.trim().is_empty()
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

pub fn merge_headers(
    forwarded: &crate::provider::types::HeaderMap,
    overrides: &[(&str, String)],
) -> crate::provider::types::HeaderMap {
    let mut merged: HashMap<String, String> = forwarded.clone();
    for (name, value) in overrides {
        if !value.trim().is_empty() {
            merged.insert(name.to_ascii_lowercase(), value.clone());
        }
    }
    merged
}

fn header_blocked(name: &str, blocklist: &[&str]) -> bool {
    blocklist
        .iter()
        .any(|blocked| name.eq_ignore_ascii_case(blocked))
}

pub fn apply_map_headers(
    headers: &mut crate::provider::types::HeaderMap,
    extra: &std::collections::HashMap<String, String>,
) {
    for (name, value) in extra {
        let value = value.trim();
        if !value.is_empty() {
            headers.insert(name.to_ascii_lowercase(), value.to_string());
        }
    }
}

pub fn apply_optional_map_headers(
    headers: &mut crate::provider::types::HeaderMap,
    extra: Option<&std::collections::HashMap<String, String>>,
) {
    if let Some(extra) = extra {
        apply_map_headers(headers, extra);
    }
}
