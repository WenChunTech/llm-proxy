use crate::{config::ProviderConfig, provider::types::AttemptTarget};

/// Credential slots for one provider config entry.
///
/// - Codex/Grok: each enabled auth entry is one slot; non-empty `api_key` adds one more.
/// - All other providers: one slot per config (single `api_key`).
///
/// Multiple keys for non-OAuth providers are modeled as multiple provider configs.
pub fn credential_slot_count(target: &AttemptTarget) -> usize {
    use crate::provider::oauth::oauth_credential_count;

    match &target.config {
        ProviderConfig::Codex(config) => {
            oauth_credential_count(&config.base.api_key, &config.auth).max(1)
        }
        ProviderConfig::Grok(config) => {
            oauth_credential_count(&config.base.api_key, &config.auth).max(1)
        }
        _ => 1,
    }
}

/// Credential slots to exercise during dashboard model testing.
///
/// For Codex/Grok, a non-empty `api_key` is treated as an explicit credential choice:
/// only the api_key is tested (auth entries are not polled). Without `api_key`, every
/// enabled auth entry is still covered.
pub fn provider_test_slot_count(target: &AttemptTarget) -> usize {
    match &target.config {
        ProviderConfig::Codex(config) if !config.base.api_key.trim().is_empty() => 1,
        ProviderConfig::Grok(config) if !config.base.api_key.trim().is_empty() => 1,
        _ => credential_slot_count(target),
    }
}

/// Attempt budget for one model: full target/credential coverage, or `max_retries` if larger.
pub fn coverage_attempt_budget(targets: &[AttemptTarget], max_retries: usize) -> usize {
    let coverage = credential_coverage_attempts(targets);
    max_retries.max(coverage.max(1))
}

pub(crate) fn credential_coverage_attempts(targets: &[AttemptTarget]) -> usize {
    targets.iter().map(credential_slot_count).sum()
}
