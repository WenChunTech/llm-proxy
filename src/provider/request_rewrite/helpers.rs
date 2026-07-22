//! Shared JSON helpers used by provider dialect rewrites.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::error::ProxyError;

pub(super) fn require_object(body: &mut Value) -> Result<&mut Map<String, Value>, ProxyError> {
    body.as_object_mut()
        .ok_or_else(|| ProxyError::InvalidRequest("request body must be a JSON object".to_string()))
}

pub(super) fn remove_fields(body: &mut Value, fields: &[&str]) -> Result<(), ProxyError> {
    let obj = require_object(body)?;
    for field in fields {
        obj.remove(*field);
    }
    Ok(())
}

pub(super) fn set_bool(body: &mut Value, field: &str, value: bool) -> Result<(), ProxyError> {
    let obj = require_object(body)?;
    obj.insert(field.to_string(), Value::Bool(value));
    Ok(())
}

pub(super) fn tools_array_mut(body: &mut Value) -> Result<Option<&mut Vec<Value>>, ProxyError> {
    let obj = require_object(body)?;
    match obj.get_mut("tools") {
        None => Ok(None),
        Some(Value::Null) => Ok(None),
        Some(Value::Array(arr)) => Ok(Some(arr)),
        Some(_) => Err(ProxyError::InvalidRequest(
            "tools must be a JSON array when present".to_string(),
        )),
    }
}

pub(super) fn tool_type(tool: &Value) -> Option<&str> {
    tool.get("type").and_then(Value::as_str)
}

pub(super) fn tool_name(tool: &Value) -> Option<&str> {
    tool.get("name").and_then(Value::as_str)
}

pub(super) fn expand_namespace_tools(body: &mut Value) -> Result<(), ProxyError> {
    let Some(tools) = tools_array_mut(body)? else {
        return Ok(());
    };

    let original = std::mem::take(tools);
    let mut kept = Vec::with_capacity(original.len());
    let mut used_names = HashSet::new();

    // First pass: reserve names of non-namespace tools so expanded names
    // can resolve conflicts against the final top-level set.
    for tool in &original {
        if tool_type(tool) == Some("namespace") {
            continue;
        }
        if let Some(name) = tool_name(tool) {
            used_names.insert(name.to_string());
        }
    }

    // Second pass: preserve relative order, expanding namespaces in place.
    for tool in original {
        if tool_type(&tool) != Some("namespace") {
            kept.push(tool);
            continue;
        }

        let namespace_name = tool_name(&tool)
            .filter(|name| !name.is_empty())
            .unwrap_or("namespace");

        let Some(inner_tools) = tool.get("tools").and_then(Value::as_array) else {
            tracing::debug!(
                namespace = namespace_name,
                "dropping namespace tool without nested tools array"
            );
            continue;
        };

        for nested in inner_tools {
            match normalize_nested_tool(nested, namespace_name, &mut used_names) {
                Some(expanded) => {
                    tracing::debug!(
                        namespace = namespace_name,
                        tool_name = tool_name(&expanded).unwrap_or(""),
                        tool_type = tool_type(&expanded).unwrap_or(""),
                        "expanded namespace tool entry"
                    );
                    kept.push(expanded);
                }
                None => {
                    tracing::debug!(
                        namespace = namespace_name,
                        nested = %nested,
                        "skipped unusable nested namespace tool entry"
                    );
                }
            }
        }
    }

    *tools = kept;
    Ok(())
}

/// Convert a nested namespace tool into a top-level tools entry.
///
/// Supports:
/// - internally tagged: `{"type":"function", ...}`
/// - externally tagged: `{"Function": {...}}` / `{"Custom": {...}}`
fn normalize_nested_tool(
    nested: &Value,
    namespace_name: &str,
    used_names: &mut HashSet<String>,
) -> Option<Value> {
    let mut tool = match nested {
        Value::Object(obj) => {
            if obj.contains_key("type") {
                Value::Object(obj.clone())
            } else if obj.len() == 1 {
                let (tag, inner) = obj.iter().next()?;
                let mut inner_obj = inner.as_object()?.clone();
                let type_name = match tag.as_str() {
                    "Function" => "function",
                    "Custom" => "custom",
                    other => {
                        tracing::debug!(
                            namespace = namespace_name,
                            variant = other,
                            "unsupported externally-tagged nested tool variant"
                        );
                        return None;
                    }
                };
                inner_obj.insert("type".to_string(), Value::String(type_name.to_string()));
                Value::Object(inner_obj)
            } else {
                return None;
            }
        }
        _ => return None,
    };

    let obj = tool.as_object_mut()?;

    let base_name = obj
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or("tool")
        .to_string();

    let unique = unique_tool_name(&base_name, namespace_name, used_names);
    obj.insert("name".to_string(), Value::String(unique));
    Some(tool)
}

fn unique_tool_name(
    base_name: &str,
    namespace_name: &str,
    used_names: &mut HashSet<String>,
) -> String {
    if used_names.insert(base_name.to_string()) {
        return base_name.to_string();
    }

    let prefixed = format!("{namespace_name}__{base_name}");
    if used_names.insert(prefixed.clone()) {
        return prefixed;
    }

    let mut index = 2usize;
    loop {
        let candidate = format!("{namespace_name}__{base_name}_{index}");
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

pub(super) fn map_tool_types(body: &mut Value, map: &[(&str, &str)]) -> Result<(), ProxyError> {
    if map.is_empty() {
        return Ok(());
    }
    let Some(tools) = tools_array_mut(body)? else {
        return Ok(());
    };

    for tool in tools.iter_mut() {
        let Some(obj) = tool.as_object_mut() else {
            continue;
        };
        let Some(current) = obj.get("type").and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        if let Some((_, to)) = map.iter().find(|(from, _)| *from == current) {
            tracing::debug!(
                from = %current,
                to = %to,
                tool_name = obj.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                "mapped tool type"
            );
            obj.insert("type".to_string(), Value::String((*to).to_string()));
        }
    }
    Ok(())
}

pub(super) fn allow_tool_types(body: &mut Value, allowed: &[&str]) -> Result<(), ProxyError> {
    let allowed: HashSet<&str> = allowed.iter().copied().collect();
    let obj = require_object(body)?;
    let Some(tools_value) = obj.get("tools") else {
        return Ok(());
    };
    if tools_value.is_null() {
        obj.remove("tools");
        return Ok(());
    }
    let original = tools_value
        .as_array()
        .ok_or_else(|| {
            ProxyError::InvalidRequest("tools must be a JSON array when present".to_string())
        })?
        .clone();

    let mut kept = Vec::with_capacity(original.len());
    for tool in original {
        match tool_type(&tool) {
            Some(ty) if allowed.contains(ty) => kept.push(tool),
            Some(ty) => {
                tracing::debug!(
                    tool_type = %ty,
                    tool_name = tool_name(&tool).unwrap_or(""),
                    "dropping unsupported tool type"
                );
            }
            None => {
                tracing::debug!(tool = %tool, "dropping tool without type");
            }
        }
    }

    if kept.is_empty() {
        obj.remove("tools");
    } else {
        obj.insert("tools".to_string(), Value::Array(kept));
    }
    Ok(())
}
