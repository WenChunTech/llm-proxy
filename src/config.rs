use std::fs;

use serde::{Deserialize, Serialize};

const CONFIG_LOCAL_FILE: &str = "config.local.json";
const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gemini: Option<Vec<GeminiConfig>>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            proxy: None,
            gemini: None,
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeminiConfig {
    pub project_id: String,
    pub token: Token,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub expire_at: u64,
    pub token_type: String,
    // pub id_token: String,
}

pub fn load_config() -> Config {
    if fs::exists(CONFIG_LOCAL_FILE).unwrap() {
        let config = fs::read_to_string(CONFIG_LOCAL_FILE).unwrap();
        serde_json::from_str(&config).unwrap()
    } else {
        let config = fs::read_to_string(CONFIG_FILE).unwrap();
        serde_json::from_str(&config).unwrap()
    }
}

pub fn save_config(config: &Config) {
    let config = serde_json::to_string_pretty(config).expect("Failed to serialize config");
    fs::write(CONFIG_LOCAL_FILE, config).expect("Failed to write config");
}
