//! Grok (xAI) request dialect rewrite.
//!
//! Grok speaks its own native wire protocol (see `converter::models::grok`).
//! The proxy applies two dialect adjustments:
//! - strip `input` items that carry empty identifiers (`function_call` with
//!   empty `call_id`/`name`, `function_call_output` with empty `call_id`);
//! - inject a bare `x_search` tool when the client already sent a `tools`
//!   array, so Grok can search X alongside client-provided tools.
//!
//! See https://docs.x.ai/developers/tools/x-search

use serde_json::{Value, json};

use crate::error::ProxyError;

use super::helpers::{filter_empty_input_items, tools_array_mut};

/// Grok dialect: strip empty-identifier input items and ensure `x_search`.
pub(super) fn rewrite(mut body: Value) -> Result<Value, ProxyError> {
    filter_empty_input_items(&mut body)?;
    ensure_x_search_tool(&mut body)?;
    Ok(body)
}

/// Ensure a bare `x_search` tool is present on Grok rewrite requests.
///
/// Only injects when the client already sent a `tools` array. Does not create a
/// tools list on tool-less requests. Existing `x_search` entries (including
/// parameterized ones) are left alone.
fn ensure_x_search_tool(body: &mut Value) -> Result<(), ProxyError> {
    let Some(tools) = tools_array_mut(body)? else {
        return Ok(());
    };

    let has_x_search = tools
        .iter()
        .any(|tool| tool.get("type").and_then(Value::as_str) == Some("x_search"));
    if has_x_search {
        return Ok(());
    }

    tracing::debug!("injecting bare x_search tool for grok rewrite");
    tools.push(json!({ "type": "x_search" }));
    Ok(())
}
