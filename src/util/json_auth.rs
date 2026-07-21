use serde_json::{Map, Value};

pub fn auth_string(auth: &Value, key: &str) -> Option<String> {
    auth.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn auth_disabled(auth: &Value) -> bool {
    auth.get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn auth_object_mut(auth: &mut Value) -> Option<&mut Map<String, Value>> {
    auth.as_object_mut()
}

pub fn set_auth_string(auth: &mut Value, key: &str, value: String) {
    if let Some(object) = auth_object_mut(auth) {
        object.insert(key.to_string(), Value::String(value));
    }
}

pub fn set_auth_i64(auth: &mut Value, key: &str, value: i64) {
    if let Some(object) = auth_object_mut(auth) {
        object.insert(key.to_string(), Value::Number(value.into()));
    }
}

pub fn set_auth_bool(auth: &mut Value, key: &str, value: bool) {
    if let Some(object) = auth_object_mut(auth) {
        object.insert(key.to_string(), Value::Bool(value));
    }
}
