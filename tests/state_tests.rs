use std::{fs, path::PathBuf};

use llm_proxy::config::{Config, ConfigPersist, LoadedConfig, UpstashRedis};
use llm_proxy::state::AppState;
use llm_proxy::util::{DumpHub, LogHub};

#[tokio::test]
async fn update_config_creates_missing_config_file_and_updates_runtime_state() {
    let path = unique_config_path("update-config-creates-missing-file");
    let parent = path.parent().unwrap().to_path_buf();
    let state = AppState::from_loaded(
        LoadedConfig {
            config: Config::default(),
            persist: ConfigPersist::File(path.clone()),
        },
        LogHub::new(),
        DumpHub::new(),
    )
    .unwrap();
    let next = Config {
        port: 4567,
        api_key: Some("proxy-key".to_string()),
        ..Default::default()
    };

    state.update_config(next).await.unwrap();

    let saved: Config = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let snapshot = state.snapshot().await;
    assert_eq!(saved.port, 4567);
    assert_eq!(saved.api_key.as_deref(), Some("proxy-key"));
    assert_eq!(snapshot.config.port, 4567);
    assert_eq!(snapshot.config.api_key.as_deref(), Some("proxy-key"));

    let _ = fs::remove_dir_all(parent);
}

#[tokio::test]
async fn update_config_writes_to_redis_when_redis_persist_is_configured() {
    let redis = UpstashRedis::for_test(
        "https://example.upstash.io",
        "token",
        "llm-proxy:config",
        Some(r#"{"port": 1}"#.to_string()),
    );
    let state = AppState::from_loaded(
        LoadedConfig {
            config: Config::default(),
            persist: ConfigPersist::Redis(redis.clone()),
        },
        LogHub::new(),
        DumpHub::new(),
    )
    .unwrap();
    let next = Config {
        port: 7654,
        api_key: Some("redis-key".to_string()),
        ..Default::default()
    };

    state.update_config(next).await.unwrap();

    let raw = redis.get().await.unwrap().expect("redis value");
    let saved: Config = serde_json::from_str(&raw).unwrap();
    let snapshot = state.snapshot().await;
    assert_eq!(saved.port, 7654);
    assert_eq!(saved.api_key.as_deref(), Some("redis-key"));
    assert_eq!(snapshot.config.port, 7654);
}

fn unique_config_path(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("llm-proxy-{name}-{}-{nanos}", std::process::id()))
        .join("config.json")
}
