use crate::{
    error::ProxyError,
    middleware::headers::{apply_map_headers, merge_headers},
    provider::types::HeaderMap,
};

/// Shared request-header assembly for API-key providers.
pub(super) fn api_key_headers(
    forwarded: &HeaderMap,
    base: &[(&str, String)],
    extra: &std::collections::HashMap<String, String>,
) -> HeaderMap {
    let mut headers = merge_headers(forwarded, base);
    apply_map_headers(&mut headers, extra);
    headers
}

pub(super) fn bearer_json_headers(
    forwarded: &HeaderMap,
    api_key: &str,
    extra: &std::collections::HashMap<String, String>,
) -> HeaderMap {
    api_key_headers(
        forwarded,
        &[
            ("content-type", "application/json".to_string()),
            ("authorization", format!("Bearer {api_key}")),
        ],
        extra,
    )
}

pub(super) fn post_json(
    client: &reqwest::Client,
    url: String,
    headers: &HeaderMap,
    body: &serde_json::Value,
) -> Result<reqwest::RequestBuilder, ProxyError> {
    Ok(client
        .post(url)
        .headers(super::reqwest_headers(headers)?)
        .json(body))
}
