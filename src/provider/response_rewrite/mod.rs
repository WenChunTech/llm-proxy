//! Provider-scoped response preparation: rewrite → convert.
//!
//! Pipeline rules:
//! - Rewrite when the upstream source declares a response dialect profile.
//! - Convert only when the upstream wire protocol differs from the client target.
//! - If neither is needed, the body is left unchanged.
//!
//! Mirrors [`crate::provider::request_rewrite`]: profiles are provider-keyed so
//! future vendor-specific response dialects can be added beside the shared wire
//! shape (e.g. Codex/Grok on Responses).

mod chat;
mod helpers;

use serde_json::Value;

use crate::{error::ProxyError, protocol, provider::types::ProviderType};

type RewriteFn = fn(Value) -> Result<Value, ProxyError>;

/// Upstream response-preparation profile (provider dimension, not op lists).
#[derive(Clone, Copy)]
struct ProviderResponseProfile {
    /// Protocol shape of the upstream response body.
    wire: ProviderType,
    /// Vendor/wire dialect rewrite applied before protocol conversion.
    rewrite: Option<RewriteFn>,
}

fn profile_for(source: ProviderType) -> ProviderResponseProfile {
    match source {
        ProviderType::Chat => ProviderResponseProfile {
            wire: ProviderType::Chat,
            rewrite: Some(chat::rewrite),
        },
        ProviderType::Responses => ProviderResponseProfile {
            wire: ProviderType::Responses,
            rewrite: None,
        },
        ProviderType::Claude => ProviderResponseProfile {
            wire: ProviderType::Claude,
            rewrite: None,
        },
        ProviderType::Gemini => ProviderResponseProfile {
            wire: ProviderType::Gemini,
            rewrite: None,
        },
        ProviderType::Codex => ProviderResponseProfile {
            wire: ProviderType::Responses,
            rewrite: None,
        },
        ProviderType::Grok => ProviderResponseProfile {
            wire: ProviderType::Grok,
            rewrite: None,
        },
    }
}

/// Prepare an upstream response for a client `target` protocol.
///
/// Steps (each optional):
/// 1. `rewrite` when the source profile declares a response dialect
/// 2. `convert` when `source.wire != target`
pub fn prepare_response(
    source: ProviderType,
    mut body: Value,
    target: ProviderType,
) -> Result<Value, ProxyError> {
    let profile = profile_for(source);
    let need_rewrite = profile.rewrite.is_some();
    let need_convert = profile.wire != target;

    tracing::debug!(
        source = ?source,
        target = ?target,
        wire = ?profile.wire,
        need_rewrite,
        need_convert,
        "preparing upstream response"
    );

    if need_rewrite && let Some(rewrite) = profile.rewrite {
        body = rewrite(body)?;
    }

    if need_convert {
        body = protocol::convert_response(body, profile.wire, target)?;
    }

    Ok(body)
}

/// Apply only the upstream response dialect rewrite (body must already be wire-shaped).
///
/// Intended for stream conversion and callers that convert separately.
pub fn rewrite_response(source: ProviderType, body: Value) -> Result<Value, ProxyError> {
    match profile_for(source).rewrite {
        Some(rewrite) => rewrite(body),
        None => Ok(body),
    }
}

/// Whether `source` declares a non-empty response dialect rewrite.
pub fn has_response_rewrite(source: ProviderType) -> bool {
    profile_for(source).rewrite.is_some()
}

/// Wire protocol for `source` responses (convert source).
pub fn response_wire_protocol(source: ProviderType) -> ProviderType {
    profile_for(source).wire
}
