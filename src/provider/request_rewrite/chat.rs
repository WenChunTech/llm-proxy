//! OpenAI Chat wire compatibility normalization.
//!
//! `openai_chat` is a generic Chat Completions-compatible wire: it targets many
//! third-party OpenAI-compatible backends (e.g. GLM/SenseNova, OpenAI-compatible
//! gateways) in addition to OpenAI itself. Several of those backends reject the
//! `developer` message role that OpenAI introduced for newer reasoning models,
//! accepting only `system | assistant | user | tool | function`.
//!
//! The `developer` role is the renamed `system` message (same semantics), and
//! `system` is universally accepted by OpenAI-compatible backends. Normalize
//! `developer` -> `system` on the Chat wire so a request authored with the
//! `developer` role succeeds across the widest set of providers.
//!
//! This runs as an always-on compatibility step (applied to both same-protocol
//! passthrough and cross-protocol conversion output), distinct from the
//! same-protocol-only dialect [`rewrite`](super::rewrite).

use serde_json::Value;

use crate::error::ProxyError;

use super::helpers::require_object;

/// Normalize Chat wire messages for broad provider compatibility.
///
/// Maps `developer` message roles to `system`; all other roles are untouched.
/// Leaves requests without a `messages` array unchanged.
pub(super) fn normalize_roles(mut body: Value) -> Result<Value, ProxyError> {
    normalize_roles_in_place(&mut body)?;
    Ok(body)
}

fn normalize_roles_in_place(body: &mut Value) -> Result<(), ProxyError> {
    let obj = require_object(body)?;
    let Some(Value::Array(messages)) = obj.get_mut("messages") else {
        return Ok(());
    };
    for message in messages.iter_mut() {
        let Some(role) = message.as_object_mut().and_then(|m| m.get_mut("role")) else {
            continue;
        };
        if role.as_str() == Some("developer") {
            *role = Value::String("system".to_string());
        }
    }
    Ok(())
}
