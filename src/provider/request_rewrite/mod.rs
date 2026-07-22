//! Provider-scoped request preparation: convert → rewrite → common.
//!
//! Pipeline rules:
//! - Convert only when the client protocol differs from the upstream wire protocol.
//! - Rewrite only when the upstream provider declares a dialect profile.
//! - If neither is needed, the body is left unchanged (aside from common flags).
//! - When both are needed, convert always runs before rewrite so dialect rules
//!   see a wire-shaped body regardless of entry endpoint.

mod codex;
mod grok;
mod helpers;

use serde_json::Value;

use crate::{error::ProxyError, protocol, provider::types::ProviderType};

type RewriteFn = fn(Value) -> Result<Value, ProxyError>;

/// Upstream request-preparation profile (provider dimension, not op lists).
#[derive(Clone, Copy)]
struct ProviderRequestProfile {
    /// Protocol shape expected by the upstream HTTP API.
    wire: ProviderType,
    /// Vendor dialect rewrite applied only after the body is wire-shaped.
    rewrite: Option<RewriteFn>,
    /// Whether this provider accepts a top-level `stream` flag.
    sets_stream: bool,
}

fn profile_for(provider: ProviderType) -> ProviderRequestProfile {
    match provider {
        ProviderType::Chat => ProviderRequestProfile {
            wire: ProviderType::Chat,
            rewrite: None,
            sets_stream: true,
        },
        ProviderType::Responses => ProviderRequestProfile {
            wire: ProviderType::Responses,
            rewrite: None,
            sets_stream: true,
        },
        ProviderType::Claude => ProviderRequestProfile {
            wire: ProviderType::Claude,
            rewrite: None,
            sets_stream: true,
        },
        ProviderType::Gemini => ProviderRequestProfile {
            wire: ProviderType::Gemini,
            rewrite: None,
            sets_stream: false,
        },
        ProviderType::Codex => ProviderRequestProfile {
            wire: ProviderType::Responses,
            rewrite: Some(codex::rewrite),
            sets_stream: true,
        },
        ProviderType::Grok => ProviderRequestProfile {
            wire: ProviderType::Responses,
            rewrite: Some(grok::rewrite),
            sets_stream: true,
        },
    }
}

/// Prepare a request for `upstream` from a client body in `source` protocol.
///
/// Steps (each optional):
/// 1. `convert` when `source != upstream.wire`
/// 2. `rewrite` when the upstream profile has a dialect
/// 3. common flags (`stream`, …)
pub fn prepare_request(
    upstream: ProviderType,
    mut body: Value,
    source: ProviderType,
    is_streaming: bool,
) -> Result<Value, ProxyError> {
    let profile = profile_for(upstream);
    let need_convert = source != profile.wire;
    let need_rewrite = profile.rewrite.is_some();

    tracing::debug!(
        upstream = ?upstream,
        source = ?source,
        wire = ?profile.wire,
        need_convert,
        need_rewrite,
        "preparing upstream request"
    );

    if need_convert {
        body = protocol::convert_request(body, source, profile.wire)?;
    }

    if let Some(rewrite) = profile.rewrite {
        body = rewrite(body)?;
    }

    if profile.sets_stream
        && let Some(obj) = body.as_object_mut()
    {
        obj.insert("stream".to_string(), Value::Bool(is_streaming));
    }

    Ok(body)
}

/// Apply only the upstream provider dialect rewrite (body must already be wire-shaped).
///
/// Intended for tests and callers that have already converted.
pub fn rewrite_request(upstream: ProviderType, body: Value) -> Result<Value, ProxyError> {
    match profile_for(upstream).rewrite {
        Some(rewrite) => rewrite(body),
        None => Ok(body),
    }
}

/// Whether `upstream` declares a non-empty dialect rewrite.
pub fn has_rewrite(upstream: ProviderType) -> bool {
    profile_for(upstream).rewrite.is_some()
}

/// Wire protocol for `upstream` (convert target).
pub fn wire_protocol(upstream: ProviderType) -> ProviderType {
    profile_for(upstream).wire
}
