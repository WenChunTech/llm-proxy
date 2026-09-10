//! Codex Responses dialect rewrite.

use serde_json::Value;

use crate::error::ProxyError;

use super::helpers::{filter_empty_input_items, remove_fields, set_bool};

/// Codex dialect: strip unsupported generation params, force `store=false`, and
/// remove `input` items with empty identifiers.
pub(super) fn rewrite(mut body: Value) -> Result<Value, ProxyError> {
    remove_fields(&mut body, &["max_output_tokens", "temperature"])?;
    set_bool(&mut body, "store", false)?;
    filter_empty_input_items(&mut body)?;
    Ok(body)
}
