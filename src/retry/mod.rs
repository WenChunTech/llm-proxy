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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_fallback_tail_for_configured_model() {
        let config = Config {
            fallback_models: vec!["a".into(), "b".into(), "c".into()],
            ..Default::default()
        };
        assert_eq!(fallback_chain("a", &config).unwrap(), vec!["b", "c"]);
        assert_eq!(fallback_chain("b", &config).unwrap(), vec!["c"]);
        assert!(fallback_chain("c", &config).unwrap().is_empty());
        assert!(fallback_chain("x", &config).unwrap().is_empty());
    }
}
