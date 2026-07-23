use std::sync::Arc;

use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::error::ProxyError;

const DEFAULT_CONFIG_KEY: &str = "llm-proxy:config";

/// Minimal Upstash Redis REST client used as a config backend.
///
/// Env vars:
/// - `UPSTASH_REDIS_REST_URL` (required to enable Redis)
/// - `UPSTASH_REDIS_REST_TOKEN` (required with URL)
/// - `UPSTASH_REDIS_CONFIG_KEY` (optional, defaults to `llm-proxy:config`)
///
/// Response shape follows the [Upstash REST API](https://upstash.com/docs/redis/features/restapi):
/// - success: `{ "result": <null|number|string|array> }`
/// - failure: `{ "error": "..." }`
/// - pipeline `/pipeline`: `[{ "result": ... } | { "error": ... }, ...]`
#[derive(Debug, Clone)]
pub struct UpstashRedis {
    pub url: String,
    pub token: String,
    pub key: String,
    http: reqwest::Client,
    /// Test-only in-memory value. When set, real HTTP is skipped.
    test_value: Option<Arc<Mutex<Option<String>>>>,
}

impl UpstashRedis {
    pub fn from_env() -> Result<Option<Self>, ProxyError> {
        let url = match std::env::var("UPSTASH_REDIS_REST_URL") {
            Ok(value) => {
                let value = value.trim().to_string();
                if value.is_empty() {
                    return Ok(None);
                }
                value
            }
            Err(_) => return Ok(None),
        };
        let token = std::env::var("UPSTASH_REDIS_REST_TOKEN")
            .map_err(|_| {
                ProxyError::Config(
                    "UPSTASH_REDIS_REST_TOKEN is required when UPSTASH_REDIS_REST_URL is set"
                        .to_string(),
                )
            })?
            .trim()
            .to_string();
        if token.is_empty() {
            return Err(ProxyError::Config(
                "UPSTASH_REDIS_REST_TOKEN must not be empty when UPSTASH_REDIS_REST_URL is set"
                    .to_string(),
            ));
        }
        let key = std::env::var("UPSTASH_REDIS_CONFIG_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_CONFIG_KEY.to_string());

        Ok(Some(Self {
            url: url.trim_end_matches('/').to_string(),
            token,
            key,
            http: reqwest::Client::new(),
            test_value: None,
        }))
    }

    pub fn for_test(url: &str, token: &str, key: &str, initial: Option<String>) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            key: key.to_string(),
            http: reqwest::Client::new(),
            test_value: Some(Arc::new(Mutex::new(initial))),
        }
    }

    pub async fn get(&self) -> Result<Option<String>, ProxyError> {
        if let Some(store) = &self.test_value {
            return Ok(store.lock().await.clone());
        }

        let response = self
            .http
            .post(&self.url)
            .bearer_auth(&self.token)
            .json(&json!(["GET", self.key]))
            .send()
            .await
            .map_err(|error| {
                ProxyError::Config(format!("upstash redis GET request failed: {error}"))
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| {
            ProxyError::Config(format!("upstash redis GET body read failed: {error}"))
        })?;
        ensure_http_ok("GET", status, &body)?;

        parse_get_result(&body)
    }

    pub async fn set(&self, value: &str) -> Result<(), ProxyError> {
        if let Some(store) = &self.test_value {
            *store.lock().await = Some(value.to_string());
            return Ok(());
        }

        let response = self
            .http
            .post(&self.url)
            .bearer_auth(&self.token)
            .json(&json!(["SET", self.key, value]))
            .send()
            .await
            .map_err(|error| {
                ProxyError::Config(format!("upstash redis SET request failed: {error}"))
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| {
            ProxyError::Config(format!("upstash redis SET body read failed: {error}"))
        })?;
        ensure_http_ok("SET", status, &body)?;

        parse_set_result(&body)
    }
}

/// Single-command REST payload: either success (`result`) or failure (`error`).
///
/// Matches Upstash docs:
/// - `{ "result": null | number | string | array }`
/// - `{ "error": "..." }`
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CommandResponse {
    Ok { result: Value },
    Err { error: String },
}

/// REST body may be a single command object or a pipeline array of command objects.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RestBody {
    Single(CommandResponse),
    Pipeline(Vec<CommandResponse>),
}

fn decode_rest_body(body: &str, op: &str) -> Result<CommandResponse, ProxyError> {
    let payload: RestBody = serde_json::from_str(body).map_err(|error| {
        ProxyError::Config(format!("invalid upstash redis {op} response: {error}"))
    })?;

    match payload {
        RestBody::Single(response) => Ok(response),
        RestBody::Pipeline(responses) => responses.into_iter().next().ok_or_else(|| {
            ProxyError::Config(format!(
                "invalid upstash redis {op} response: empty pipeline"
            ))
        }),
    }
}

fn ensure_http_ok(op: &str, status: StatusCode, body: &str) -> Result<(), ProxyError> {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(ProxyError::Config(format!(
            "upstash redis {op} unauthorized ({status}): {body}"
        )));
    }
    if !status.is_success() {
        // Prefer structured error field when present (HTTP 400 paths).
        if let Ok(CommandResponse::Err { error }) = decode_rest_body(body, op) {
            return Err(ProxyError::Config(format!(
                "upstash redis {op} failed with status {status}: {error}"
            )));
        }
        return Err(ProxyError::Config(format!(
            "upstash redis {op} failed with status {status}: {body}"
        )));
    }
    Ok(())
}

pub fn parse_get_result(body: &str) -> Result<Option<String>, ProxyError> {
    match decode_rest_body(body, "GET")? {
        CommandResponse::Err { error } => Err(ProxyError::Config(format!(
            "upstash redis GET error: {error}"
        ))),
        CommandResponse::Ok {
            result: Value::Null,
        } => Ok(None),
        CommandResponse::Ok {
            result: Value::String(raw),
        } if raw.trim().is_empty() => Ok(None),
        CommandResponse::Ok {
            result: Value::String(raw),
        } => Ok(Some(raw)),
        CommandResponse::Ok { result: other } => Err(ProxyError::Config(format!(
            "invalid upstash redis GET result type: {other}"
        ))),
    }
}

pub fn parse_set_result(body: &str) -> Result<(), ProxyError> {
    match decode_rest_body(body, "SET")? {
        CommandResponse::Err { error } => Err(ProxyError::Config(format!(
            "upstash redis SET error: {error}"
        ))),
        CommandResponse::Ok {
            result: Value::String(status),
        } if status.eq_ignore_ascii_case("ok") => Ok(()),
        CommandResponse::Ok { result: other } => Err(ProxyError::Config(format!(
            "unexpected upstash redis SET result: {other}"
        ))),
    }
}
