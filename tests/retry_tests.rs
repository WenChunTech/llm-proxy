use llm_proxy::config::Config;
use llm_proxy::retry::{fallback_chain, models_to_try};
use std::collections::HashMap;

#[test]
fn models_to_try_starts_with_requested_then_aliases_then_global_fallbacks() {
    let config = Config {
        fallback_models: vec!["global-a".into(), "global-b".into()],
        model_aliases: HashMap::from([(
            "primary".into(),
            vec!["alias-1".into(), "alias-2".into()],
        )]),
        ..Default::default()
    };

    assert_eq!(
        models_to_try("primary", &config).unwrap(),
        vec!["primary", "alias-1", "alias-2", "global-a", "global-b"]
    );
    assert_eq!(
        fallback_chain("primary", &config).unwrap(),
        vec!["alias-1", "alias-2", "global-a", "global-b"]
    );
}

#[test]
fn models_to_try_skips_duplicates_across_aliases_and_fallbacks() {
    let config = Config {
        fallback_models: vec!["alias-2".into(), "global-a".into(), "primary".into()],
        model_aliases: HashMap::from([(
            "primary".into(),
            vec!["alias-1".into(), "alias-2".into()],
        )]),
        ..Default::default()
    };

    assert_eq!(
        models_to_try("primary", &config).unwrap(),
        vec!["primary", "alias-1", "alias-2", "global-a"]
    );
}

#[test]
fn models_without_aliases_still_use_global_fallbacks() {
    let config = Config {
        fallback_models: vec!["a".into(), "b".into()],
        ..Default::default()
    };
    assert_eq!(
        models_to_try("x", &config).unwrap(),
        vec!["x", "a", "b"]
    );
    assert_eq!(fallback_chain("a", &config).unwrap(), vec!["b"]);
}
