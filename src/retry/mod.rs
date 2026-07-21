use crate::{config::Config, error::ProxyError};

pub fn fallback_chain(model: &str, config: &Config) -> Result<Vec<String>, ProxyError> {
    let chain = &config.fallback_models;
    let Some(start) = chain.iter().position(|candidate| candidate == model) else {
        return Ok(Vec::new());
    };

    Ok(chain.iter().skip(start + 1).cloned().collect())
}

pub fn backoff_delay_ms(attempt: usize, config: &Config) -> u64 {
    attempt as u64 * config.retry.backoff_step_ms
}

pub fn should_attempt(attempts: usize, max_retries: usize) -> bool {
    attempts < max_retries
}
