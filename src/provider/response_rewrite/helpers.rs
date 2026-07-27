//! Shared JSON helpers used by response dialect rewrites.

use serde_json::{Map, Value};

pub(super) fn choices_array_mut(body: &mut Value) -> Option<&mut Vec<Value>> {
    body.get_mut("choices").and_then(Value::as_array_mut)
}

pub(super) fn normalize_empty_string_field(obj: &mut Map<String, Value>, field: &str) {
    match obj.get(field) {
        Some(Value::String(value)) if value.is_empty() => {
            obj.insert(field.to_string(), Value::Null);
        }
        _ => {}
    }
}
