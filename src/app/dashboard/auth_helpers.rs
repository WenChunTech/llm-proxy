use serde_json::Value;

pub(super) fn auth_access_token(auth_json: Option<&Value>) -> Option<&str> {
    auth_values(auth_json)
        .into_iter()
        .filter(|item| {
            !item
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .find_map(|item| item.get("access_token").and_then(Value::as_str))
        .filter(|token| !token.trim().is_empty())
}

pub(super) fn auth_headers(auth_json: Option<&Value>) -> Option<Vec<(&str, &str)>> {
    let selected_auth = auth_values(auth_json).into_iter().find(|item| {
        !item
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    })?;
    let headers = selected_auth.get("headers")?.as_object()?;
    let mut out = Vec::new();
    for (name, value) in headers {
        let Some(value) = value.as_str().filter(|value| !value.trim().is_empty()) else {
            continue;
        };
        out.push((name.as_str(), value));
    }
    Some(out)
}

pub(super) fn auth_values(auth_json: Option<&Value>) -> Vec<&Value> {
    match auth_json {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Object(_)) => auth_json.into_iter().collect(),
        _ => Vec::new(),
    }
}
