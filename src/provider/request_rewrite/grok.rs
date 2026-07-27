//! Grok (xAI) request dialect rewrite.
//!
//! Grok speaks its own native wire protocol (see `converter::models::grok`).
//! The only dialect adjustment the proxy applies is injecting a bare `x_search`
//! tool when the client already sent a `tools` array, so Grok can search X
//! alongside the client-provided tools.
//!
//! See https://docs.x.ai/developers/tools/x-search

use serde_json::{Value, json};

use crate::error::ProxyError;

use super::helpers::tools_array_mut;

/// Grok dialect: ensure a bare `x_search` tool is present when tools are sent.
pub(super) fn rewrite(mut body: Value) -> Result<Value, ProxyError> {
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
