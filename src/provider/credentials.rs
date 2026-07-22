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

/// Attempt budget for one model: full target/credential coverage, or `max_retries` if larger.
pub fn coverage_attempt_budget(targets: &[AttemptTarget], max_retries: usize) -> usize {
    let coverage = credential_coverage_attempts(targets);
    max_retries.max(coverage.max(1))
}

pub(crate) fn credential_coverage_attempts(targets: &[AttemptTarget]) -> usize {
    targets.iter().map(credential_slot_count).sum()
}
