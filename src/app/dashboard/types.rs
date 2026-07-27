use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::provider::UpstreamResponse;

#[derive(Debug, Clone, Serialize)]
pub(super) struct DashboardPayload {
    pub port: u16,
    pub providers: Vec<DashboardProvider>,
    pub model_priority: Vec<String>,
    pub fallback_models: Vec<String>,
    pub model_aliases: std::collections::HashMap<String, String>,
    pub retry: DashboardRetry,
    pub api_key: String,
    pub api_key_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
    pub debug_dump: DashboardDebugDump,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct DashboardConfig {
    #[serde(default)]
    pub port: Option<u16>,
    pub providers: Vec<DashboardProvider>,
    pub model_priority: Vec<String>,
    pub fallback_models: Vec<String>,
    #[serde(default)]
    pub model_aliases: std::collections::HashMap<String, String>,
    pub retry: DashboardRetry,
    #[serde(default)]
    pub api_key: Option<String>,
    /// When present, replaces the configured log level. Empty string clears it.
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub debug_dump: Option<DashboardDebugDump>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct DashboardProvider {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ProviderModelsRequest {
    pub kind: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub auth: Option<Value>,
}

#[derive(Debug, Clone)]
pub(super) struct ProviderModelsResult {
    pub endpoint: String,
    pub model_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ProviderTestRequest {
    pub provider: DashboardProvider,
    pub model: String,
    pub prompt: Option<String>,
    #[serde(default = "default_provider_test_stream")]
    pub stream: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AuthValidateRequest {
    #[serde(default)]
    pub config: Option<DashboardAuthConfig>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default, alias = "providerIndices")]
    pub provider_indices: Option<Vec<usize>>,
    #[serde(default)]
    pub targets: Option<Vec<AuthValidateTarget>>,
    /// Concurrent auth probes. Defaults to 5 when omitted/invalid.
    #[serde(default, alias = "maxConcurrency")]
    pub concurrency: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct DashboardAuthConfig {
    #[serde(default)]
    pub codex: Vec<DashboardAuthProvider>,
    #[serde(default)]
    pub grok: Vec<DashboardAuthProvider>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardAuthProvider {
    #[serde(default = "default_dashboard_provider_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub auth: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AuthValidateTarget {
    #[serde(alias = "provider_index")]
    pub provider_index: usize,
    #[serde(alias = "auth_index")]
    pub auth_index: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AuthValidateResponse {
    pub success: bool,
    pub data: AuthValidatePayload,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AuthValidatePayload {
    pub model: String,
    pub provider_indices: Vec<usize>,
    pub targets: Vec<String>,
    pub total: usize,
    pub checked: usize,
    pub valid: usize,
    pub invalid: usize,
    pub skipped: usize,
    pub rate_limited: usize,
    pub refreshed: usize,
    pub results: Vec<AuthValidateResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AuthValidateResult {
    pub provider_index: usize,
    pub auth_index: usize,
    pub auth_count: usize,
    pub is_auth_array: bool,
    pub label: String,
    pub disabled: bool,
    pub skipped: bool,
    pub valid: bool,
    pub reason: String,
    pub status_code: u16,
    pub error_message: String,
    pub refreshed: bool,
    pub auth: Value,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ProviderTestResult {
    pub ok: bool,
    pub status: u16,
    pub provider: String,
    pub model: String,
    pub stream: bool,
    pub raw_body: String,
    pub body_preview: String,
}

pub(super) enum ProviderTestStreamResponse {
    Upstream(UpstreamResponse),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct DashboardRetry {
    pub max_retries: usize,
    pub backoff_step_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct DashboardDebugDump {
    pub enabled: bool,
    pub dir: String,
}

pub(super) fn default_provider_test_stream() -> bool {
    true
}

pub(super) fn default_dashboard_provider_enabled() -> bool {
    true
}
