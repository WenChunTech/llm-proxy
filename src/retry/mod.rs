use std::collections::HashSet;

use crate::{config::Config, error::ProxyError};

/// Ordered models to try for one request:
/// 1. the requested model itself
/// 2. per-model alias targets (`model_aliases[model]`)
/// 3. global fallback models (`fallback_models`)
///
/// Duplicates are skipped while preserving first-seen order.
pub fn models_to_try(model: &str, config: &Config) -> Result<Vec<String>, ProxyError> {
    let mut seen = HashSet::new();
    let mut chain = Vec::new();

    let push = |value: &str, seen: &mut HashSet<String>, chain: &mut Vec<String>| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        if seen.insert(trimmed.to_string()) {
            chain.push(trimmed.to_string());
        }
    };

    push(model, &mut seen, &mut chain);

    for target in config.alias_targets_for(model) {
        push(target, &mut seen, &mut chain);
    }

    for candidate in &config.fallback_models {
        push(candidate, &mut seen, &mut chain);
    }

    Ok(chain)
}

/// Global fallback models only (excludes the primary model).
pub fn fallback_chain(model: &str, config: &Config) -> Result<Vec<String>, ProxyError> {
    Ok(models_to_try(model, config)?
        .into_iter()
        .skip(1)
        .collect())
}

pub fn backoff_delay_ms(attempt: usize, config: &Config) -> u64 {
    attempt as u64 * config.retry.backoff_step_ms
}

pub fn should_attempt(attempts: usize, max_retries: usize) -> bool {
    attempts < max_retries
}
