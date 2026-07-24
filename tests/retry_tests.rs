use llm_proxy::config::Config;
use llm_proxy::retry::fallback_chain;

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
