//! Grok Responses dialect rewrite.
//!
//! Wire protocol is OpenAI Responses; xAI only accepts a subset of tool shapes
//! and search parameters. See:
//! - https://docs.x.ai/developers/tools/web-search
//! - https://docs.x.ai/developers/tools/x-search

use serde_json::{json, Value};

use crate::error::ProxyError;

use super::helpers::{
    allow_tool_types, expand_namespace_tools, map_tool_types, tools_array_mut,
};

/// Grok dialect: expand namespaces, normalize tool aliases, allowlist tools,
/// adapt search tools to the xAI Responses shape, and ensure `x_search`.
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
    adapt_x_search_tools(&mut body)?;
    ensure_x_search_tool(&mut body)?;
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

/// Normalize `x_search` tools to the xAI Responses shape.
///
/// Per https://docs.x.ai/developers/tools/x-search :
/// - top-level: `allowed_x_handles` / `excluded_x_handles` (max 20, mutually exclusive)
/// - `from_date` / `to_date` (ISO8601, e.g. `"YYYY-MM-DD"`)
/// - `enable_image_understanding` / `enable_video_understanding`
/// - bare `{"type":"x_search"}` is valid
///
/// CamelCase aliases from Vercel AI SDK (`allowedXHandles`, `fromDate`, …) are
/// folded into the documented snake_case keys. Unknown fields are dropped.
fn adapt_x_search_tools(body: &mut Value) -> Result<(), ProxyError> {
    const MAX_HANDLES: usize = 20;
    const KEEP_FIELDS: &[&str] = &[
        "type",
        "allowed_x_handles",
        "excluded_x_handles",
        "from_date",
        "to_date",
        "enable_image_understanding",
        "enable_video_understanding",
    ];
    // AI SDK / JS clients often send camelCase.
    const ALIASES: &[(&str, &str)] = &[
        ("allowedXHandles", "allowed_x_handles"),
        ("excludedXHandles", "excluded_x_handles"),
        ("fromDate", "from_date"),
        ("toDate", "to_date"),
        ("enableImageUnderstanding", "enable_image_understanding"),
        ("enableVideoUnderstanding", "enable_video_understanding"),
    ];

    let Some(tools) = tools_array_mut(body)? else {
        return Ok(());
    };

    for tool in tools.iter_mut() {
        let Some(obj) = tool.as_object_mut() else {
            continue;
        };
        if obj.get("type").and_then(Value::as_str) != Some("x_search") {
            continue;
        }

        for (from, to) in ALIASES {
            if obj.contains_key(*to) {
                // Canonical key wins; drop the alias.
                if obj.remove(*from).is_some() {
                    tracing::debug!(
                        from = %from,
                        to = %to,
                        "dropped aliased x_search field; canonical key already present"
                    );
                }
                continue;
            }
            if let Some(value) = obj.remove(*from) {
                tracing::debug!(from = %from, to = %to, "mapped x_search field alias");
                obj.insert((*to).to_string(), value);
            }
        }

        for key in ["allowed_x_handles", "excluded_x_handles"] {
            if let Some(Value::Array(handles)) = obj.get_mut(key)
                && handles.len() > MAX_HANDLES
            {
                tracing::debug!(
                    field = key,
                    original_len = handles.len(),
                    max = MAX_HANDLES,
                    "truncating grok x_search handle list"
                );
                handles.truncate(MAX_HANDLES);
            }
        }

        // xAI: allowed_x_handles and excluded_x_handles cannot be set together.
        // Prefer allow-list when both are present.
        if obj.contains_key("allowed_x_handles") && obj.contains_key("excluded_x_handles") {
            tracing::debug!(
                "grok x_search has both allowed_x_handles and excluded_x_handles; dropping excluded_x_handles"
            );
            obj.remove("excluded_x_handles");
        }

        let removed: Vec<String> = obj
            .keys()
            .filter(|k| !KEEP_FIELDS.contains(&k.as_str()))
            .cloned()
            .collect();
        for key in removed {
            obj.remove(&key);
            tracing::debug!(field = %key, "removed unsupported grok x_search field");
        }
    }

    Ok(())
}

/// Ensure a bare `x_search` tool is present on Grok rewrite requests.
///
/// Only injects when the client already sent a `tools` array (or one remains
/// after allowlisting). Does not create a tools list on tool-less requests.
/// Existing `x_search` entries (including parameterized ones) are left alone.
fn ensure_x_search_tool(body: &mut Value) -> Result<(), ProxyError> {
    let Some(tools) = tools_array_mut(body)? else {
        return Ok(());
    };

    let has_x_search = tools.iter().any(|tool| {
        tool.get("type").and_then(Value::as_str) == Some("x_search")
    });
    if has_x_search {
        return Ok(());
    }

    tracing::debug!("injecting bare x_search tool for grok rewrite");
    tools.push(json!({ "type": "x_search" }));
    Ok(())
}
