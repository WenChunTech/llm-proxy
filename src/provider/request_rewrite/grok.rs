//! Grok Responses dialect rewrite.
//!
//! Wire protocol is OpenAI Responses; xAI only accepts a subset of tool shapes
//! and `web_search` parameters. See https://docs.x.ai/developers/tools/web-search

use serde_json::Value;

use crate::error::ProxyError;

use super::helpers::{
    allow_tool_types, expand_namespace_tools, map_tool_types, tools_array_mut,
};

/// Grok dialect: expand namespaces, normalize tool aliases, allowlist tools,
/// and adapt `web_search` to the xAI Responses shape.
pub(super) fn rewrite(mut body: Value) -> Result<Value, ProxyError> {
    expand_namespace_tools(&mut body)?;
    map_tool_types(
        &mut body,
        &[
            ("web_search_preview", "web_search"),
            ("web_search_preview_2025_03_11", "web_search"),
            ("web_search_2025_08_26", "web_search"),
            ("local_shell", "shell"),
        ],
    )?;
    allow_tool_types(
        &mut body,
        &[
            "function",
            "web_search",
            "x_search",
            "image_generation",
            "collections_search",
            "file_search",
            "code_execution",
            "code_interpreter",
            "mcp",
            "shell",
        ],
    )?;
    adapt_web_search_tools(&mut body)?;
    Ok(body)
}

/// Normalize `web_search` tools to the xAI Responses shape.
///
/// Per https://docs.x.ai/developers/tools/web-search :
/// - domains live under `filters.allowed_domains` / `filters.excluded_domains`
///   (max 5, mutually exclusive)
/// - `enable_image_understanding` / `enable_image_search` are top-level
/// - bare `{"type":"web_search"}` is valid
///
/// OpenAI/Codex extras (`external_web_access`, `search_content_types`,
/// `search_context_size`, `user_location`, unknown filter keys) are mapped or
/// dropped. Notably `search_content_types` containing `"image"` becomes
/// `enable_image_search: true`.
fn adapt_web_search_tools(body: &mut Value) -> Result<(), ProxyError> {
    const MAX_DOMAINS: usize = 5;
    const KEEP_FIELDS: &[&str] = &[
        "type",
        "filters",
        "enable_image_understanding",
        "enable_image_search",
    ];

    let Some(tools) = tools_array_mut(body)? else {
        return Ok(());
    };

    for tool in tools.iter_mut() {
        let Some(obj) = tool.as_object_mut() else {
            continue;
        };
        if obj.get("type").and_then(Value::as_str) != Some("web_search") {
            continue;
        }

        // OpenAI: search_content_types: ["text","image"] -> enable_image_search
        if let Some(types) = obj.get("search_content_types").and_then(Value::as_array) {
            let wants_image = types.iter().any(|v| v.as_str() == Some("image"));
            if wants_image && obj.get("enable_image_search").is_none() {
                obj.insert("enable_image_search".to_string(), Value::Bool(true));
                tracing::debug!(
                    "mapped search_content_types image -> enable_image_search=true"
                );
            }
        }

        // Start from existing filters, then fold top-level domain lists into them.
        // (Some clients place domains at the top level; Responses/xAI uses filters.)
        let mut filters = obj
            .get("filters")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        if let Some(domains) = obj.remove("allowed_domains") {
            filters.insert("allowed_domains".to_string(), domains);
        }
        if let Some(domains) = obj.remove("excluded_domains") {
            filters.insert("excluded_domains".to_string(), domains);
        }

        for key in ["allowed_domains", "excluded_domains"] {
            if let Some(Value::Array(domains)) = filters.get_mut(key)
                && domains.len() > MAX_DOMAINS
            {
                tracing::debug!(
                    field = key,
                    original_len = domains.len(),
                    max = MAX_DOMAINS,
                    "truncating grok web_search domain list"
                );
                domains.truncate(MAX_DOMAINS);
            }
        }

        // xAI: allowed_domains and excluded_domains cannot be set together.
        // Prefer allow-list when both are present.
        if filters.contains_key("allowed_domains") && filters.contains_key("excluded_domains") {
            tracing::debug!(
                "grok web_search has both allowed_domains and excluded_domains; dropping excluded_domains"
            );
            filters.remove("excluded_domains");
        }

        // Keep only domain filter keys Grok documents.
        let filter_keys: Vec<String> = filters.keys().cloned().collect();
        for key in filter_keys {
            if key != "allowed_domains" && key != "excluded_domains" {
                filters.remove(&key);
                tracing::debug!(field = %key, "removed unsupported grok web_search filter field");
            }
        }

        if filters.is_empty() {
            obj.remove("filters");
        } else {
            obj.insert("filters".to_string(), Value::Object(filters));
        }

        let removed: Vec<String> = obj
            .keys()
            .filter(|k| !KEEP_FIELDS.contains(&k.as_str()))
            .cloned()
            .collect();
        for key in removed {
            obj.remove(&key);
            tracing::debug!(field = %key, "removed unsupported grok web_search field");
        }
    }

    Ok(())
}
