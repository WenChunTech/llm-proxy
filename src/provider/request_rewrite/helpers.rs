use serde_json::{Map, Value};

use crate::error::ProxyError;

/// Return the request body as a mutable object, or error if not an object.
pub(super) fn require_object(body: &mut Value) -> Result<&mut Map<String, Value>, ProxyError> {
    body.as_object_mut().ok_or_else(|| {
        ProxyError::InvalidRequest("request body must be a JSON object".to_string())
    })
}

/// Remove top-level fields from the request body.
pub(super) fn remove_fields(body: &mut Value, fields: &[&str]) -> Result<(), ProxyError> {
    let obj = require_object(body)?;
    for field in fields {
        obj.remove(*field);
    }
    Ok(())
}

/// Set a top-level boolean field on the request body.
pub(super) fn set_bool(body: &mut Value, field: &str, value: bool) -> Result<(), ProxyError> {
    let obj = require_object(body)?;
    obj.insert(field.to_string(), Value::Bool(value));
    Ok(())
}

/// Return a mutable reference to the request `tools` array when present.
pub(super) fn tools_array_mut(body: &mut Value) -> Result<Option<&mut Vec<Value>>, ProxyError> {
    let obj = require_object(body)?;
    match obj.get_mut("tools") {
        Some(Value::Array(tools)) => Ok(Some(tools)),
        _ => Ok(None),
    }
}
