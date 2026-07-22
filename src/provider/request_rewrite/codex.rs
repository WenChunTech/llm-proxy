//! Codex Responses dialect rewrite.

use serde_json::Value;

use crate::error::ProxyError;

use super::helpers::{remove_fields, set_bool};

/// Codex dialect: strip unsupported generation params and force `store=false`.
pub(super) fn rewrite(mut body: Value) -> Result<Value, ProxyError> {
    remove_fields(&mut body, &["max_output_tokens", "temperature"])?;
    set_bool(&mut body, "store", false)?;
    Ok(body)
}
