use std::{collections::HashMap, fs, path::PathBuf};

use llm_proxy::config::{
    BaseProviderConfig, CodexAuth, CodexConfig, Config, ConfigPersist, ConfigSources, GrokAuth,
    GrokConfig, OneOrMany, OpenAiChatConfig, ProviderGroups, UpstashRedis,
    load_config_from_sources, parse_config_value, validate_config,
};
use serde_json::json;

#[test]
fn port_config_is_read_directly() {
    let config = parse_config_value(json!({
        "port": 7001
    }))
    .unwrap();

    assert_eq!(config.port, 7001);
    assert_eq!(config.bind_addr(), "0.0.0.0:7001");
    assert!(!config.extra.contains_key("server"));
}

#[test]
fn legacy_server_bind_migrates_to_port() {
    let config = parse_config_value(json!({
        "server": { "bind": "0.0.0.0:7001" }
    }))
    .unwrap();

    assert_eq!(config.port, 7001);
    assert!(!config.extra.contains_key("server"));
}

#[test]
fn explicit_port_wins_over_legacy_server_bind() {
    let config = parse_config_value(json!({
        "port": 4000,
        "server": { "bind": "0.0.0.0:7001" }
    }))
    .unwrap();

    assert_eq!(config.port, 4000);
    assert!(!config.extra.contains_key("server"));
}

#[test]
fn log_level_config_is_read_directly() {
    let config = parse_config_value(json!({
        "log_level": "debug"
    }))
    .unwrap();

    assert_eq!(config.log_level.as_deref(), Some("debug"));
    validate_config(&config).unwrap();
}

#[test]
fn invalid_log_level_is_rejected() {
    let config = Config {
        log_level: Some("llm_proxy=verbose".to_string()),
        ..Default::default()
    };

    let error = validate_config(&config).unwrap_err();
    assert!(error.to_string().contains("invalid log_level"));
}

#[tokio::test]
async fn missing_default_config_uses_default_and_keeps_writable_path() {
    let default_path = missing_test_path("default");
    let loaded = load_config_from_sources(ConfigSources {
        cli_path: None,
        redis: None,
        default_path: default_path.clone(),
    })
    .await
    .unwrap();

    assert_eq!(loaded.config.port, Config::default().port);
    match loaded.persist {
        ConfigPersist::File(path) => assert_eq!(path, default_path),
        ConfigPersist::Redis(_) => panic!("expected file persist"),
    }
}

#[tokio::test]
async fn missing_cli_config_uses_default_and_keeps_cli_path() {
    let cli_path = missing_test_path("cli");
    let loaded = load_config_from_sources(ConfigSources {
        cli_path: Some(cli_path.clone()),
        redis: None,
        default_path: PathBuf::from("config.json"),
    })
    .await
    .unwrap();

    assert_eq!(loaded.config.port, Config::default().port);
    match loaded.persist {
        ConfigPersist::File(path) => assert_eq!(path, cli_path),
        ConfigPersist::Redis(_) => panic!("expected file persist"),
    }
}

#[tokio::test]
async fn redis_priority_uses_redis_value_over_file() {
    let file_path = missing_test_path("redis-over-file");
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&file_path, r#"{"port": 1111}"#).unwrap();

    let redis = UpstashRedis::for_test(
        "https://example.upstash.io",
        "token",
        "llm-proxy:config",
        Some(r#"{"port": 2222}"#.to_string()),
    );
    let loaded = load_config_from_sources(ConfigSources {
        cli_path: Some(file_path.clone()),
        redis: Some(redis),
        default_path: PathBuf::from("config.json"),
    })
    .await
    .unwrap();

    assert_eq!(loaded.config.port, 2222);
    assert!(matches!(loaded.persist, ConfigPersist::Redis(_)));
    let _ = fs::remove_dir_all(file_path.parent().unwrap());
}

#[tokio::test]
async fn redis_miss_seeds_default_config_and_ignores_config_file() {
    let file_path = missing_test_path("redis-miss-file");
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    // File content must not be used when Redis is configured but the key is missing.
    fs::write(&file_path, r#"{"port": 3333}"#).unwrap();

    let redis = UpstashRedis::for_test(
        "https://example.upstash.io",
        "token",
        "llm-proxy:config",
        None,
    );
    let loaded = load_config_from_sources(ConfigSources {
        cli_path: Some(file_path.clone()),
        redis: Some(redis.clone()),
        default_path: PathBuf::from("config.json"),
    })
    .await
    .unwrap();

    assert_eq!(loaded.config.port, Config::default().port);
    assert!(matches!(loaded.persist, ConfigPersist::Redis(_)));

    // Missing Redis key is initialized with the default config.
    let seeded = redis
        .get()
        .await
        .unwrap()
        .expect("redis key should be seeded");
    let seeded_config: Config = serde_json::from_str(&seeded).unwrap();
    assert_eq!(seeded_config.port, Config::default().port);

    let _ = fs::remove_dir_all(file_path.parent().unwrap());
}

#[test]
fn codex_auth_allows_access_token_without_refresh_token() {
    let auth: CodexAuth = serde_json::from_value(json!({
        "access_token": "access-token"
    }))
    .unwrap();

    assert_eq!(auth.access_token.as_deref(), Some("access-token"));
    assert!(auth.refresh_token.is_none());
}

#[test]
fn grok_auth_allows_access_token_without_refresh_token() {
    let auth: GrokAuth = serde_json::from_value(json!({
        "access_token": "access-token"
    }))
    .unwrap();

    assert_eq!(auth.access_token.as_deref(), Some("access-token"));
    assert!(auth.refresh_token.is_none());
}

#[test]
fn codex_config_allows_api_key_without_auth() {
    let config: CodexConfig = serde_json::from_value(json!({
        "models": ["codex-model"],
        "base_url": "https://api.example.com/v1",
        "api_key": "key"
    }))
    .unwrap();

    assert_eq!(config.base.api_key, "key");
    assert_eq!(config.base.base_url, "https://api.example.com/v1");
    assert!(matches!(config.auth, OneOrMany::Many(ref items) if items.is_empty()));
}

#[test]
fn grok_config_allows_api_key_without_auth() {
    let config: GrokConfig = serde_json::from_value(json!({
        "models": ["grok-model"],
        "base_url": "https://api.example.com/v1",
        "api_key": "key"
    }))
    .unwrap();

    assert_eq!(config.base.api_key, "key");
    assert_eq!(config.base.base_url, "https://api.example.com/v1");
    assert!(matches!(config.auth, OneOrMany::Many(ref items) if items.is_empty()));
}

#[test]
fn model_aliases_resolve_and_validate_against_provider_models() {
    let config: Config = serde_json::from_value(json!({
        "model_aliases": {
            "gpt-4o-mini": "gpt-4.1"
        },
        "providers": {
            "openai_chat": [{
                "models": ["gpt-4.1", "gpt-4o-mini"],
                "base_url": "https://api.openai.com/v1",
                "api_key": "key",
                "headers": {
                    "X-Custom": "1"
                }
            }]
        }
    }))
    .unwrap();

    assert_eq!(config.resolve_model_alias("gpt-4o-mini"), "gpt-4.1");
    assert_eq!(config.resolve_model_alias("gpt-4.1"), "gpt-4.1");
    assert_eq!(
        config.providers.openai_chat[0]
            .base
            .headers
            .get("X-Custom")
            .map(String::as_str),
        Some("1")
    );
    validate_config(&config).unwrap();

    let invalid = Config {
        model_aliases: HashMap::from([("missing".into(), "gpt-4.1".into())]),
        providers: ProviderGroups {
            openai_chat: vec![OpenAiChatConfig {
                base: BaseProviderConfig {
                    enabled: true,
                    models: vec!["gpt-4.1".into()],
                    base_url: "https://api.openai.com/v1".into(),
                    api_key: "key".into(),
                    headers: HashMap::new(),
                },
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(validate_config(&invalid).is_err());
}

#[test]
fn bun_config_provider_fields_can_be_ignored() {
    let config: Config = serde_json::from_value(json!({
        "model_priority": [
            "gemini_cli",
            "iflow",
            "openai_chat",
            "openai_responses",
            "qwen",
            "claude",
            "gemini",
            "codex",
            "grok"
        ],
        "gemini_cli": [{
            "models": ["gemini-2.5-pro"],
            "projects": ["project"],
            "auth": {
                "access_token": "token",
                "scope": "scope",
                "token_type": "Bearer",
                "expiry_date": 1,
                "refresh_token": "refresh"
            }
        }],
        "qwen": [{
            "models": ["qwen"],
            "auth": {
                "access_token": "token",
                "refresh_token": "refresh",
                "expiry_date": 1,
                "status": "ok",
                "token_type": "Bearer",
                "expires_in": 1,
                "scope": "scope",
                "resource_url": "example.com"
            }
        }],
        "iflow": [{
            "models": ["iflow"],
            "auth": {
                "access_token": "token",
                "token_type": "Bearer",
                "refresh_token": "refresh",
                "expires_in": 1,
                "scope": "scope",
                "expiry_date": 1,
                "userId": "user",
                "userName": "name",
                "avatar": "",
                "email": null,
                "phone": "",
                "apiKey": "key"
            }
        }],
        "providers": {
            "openai_chat": [{
                "models": ["gpt-4"],
                "base_url": "https://api.openai.com/v1",
                "api_key": "key"
            }]
        }
    }))
    .unwrap();

    assert_eq!(config.model_priority[0], "gemini_cli");
    assert_eq!(config.providers.openai_chat.len(), 1);
}

fn missing_test_path(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("llm-proxy-{name}-{}-{nanos}", std::process::id()))
        .join("config.json")
}

#[test]
fn debug_dump_config_defaults_and_parses() {
    let default = Config::default();
    assert!(!default.debug_dump.enabled);
    assert_eq!(default.debug_dump.dir, "logs");

    let config = parse_config_value(serde_json::json!({
        "debug_dump": {
            "enabled": true,
            "dir": "req"
        }
    }))
    .unwrap();
    assert!(config.debug_dump.enabled);
    assert_eq!(config.debug_dump.dir, "req");
}

