//! OpenAI Chat Completions response dialect rewrite.

use serde_json::Value;

use crate::error::ProxyError;

use super::helpers::{choices_array_mut, normalize_empty_string_field};

/// Chat response dialect: tolerate non-standard empty `finish_reason` values.
///
/// Some OpenAI-compatible providers emit `""` instead of `null` on intermediate
/// stream chunks; normalize those to `null` before typed conversion.
pub(super) fn rewrite(mut body: Value) -> Result<Value, ProxyError> {
    let Some(choices) = choices_array_mut(&mut body) else {
        return Ok(body);
    };

    for choice in choices {
        let Some(obj) = choice.as_object_mut() else {
            continue;
        };
        normalize_empty_string_field(obj, "finish_reason");
    }

    Ok(body)
}
