//! OpenAI Responses dialect rewrite.
//!
//! Strips `input` items that carry empty identifiers so downstream providers do
//! not receive malformed function-call/output entries (e.g. a
//! `function_call` with an empty `call_id` or `name`, or a
//! `function_call_output` with an empty `call_id`).

use serde_json::Value;

use crate::error::ProxyError;

use super::helpers::filter_empty_input_items;

/// Responses dialect: remove `input` items with empty call identifiers.
pub(super) fn rewrite(mut body: Value) -> Result<Value, ProxyError> {
    filter_empty_input_items(&mut body)?;
    Ok(body)
}
