use serde::Deserialize;
use std::fs;

use crate::error::{GymSniperError, Result};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub gym: GymConfig,
    pub credentials: Credentials,
    #[serde(default)]
    pub targets: Vec<ClassTarget>,
    pub email: Option<EmailConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EmailConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GymConfig {
    pub base_url: String,
    pub club_id: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClassTarget {
    pub class_name: String,
    pub days: Option<Vec<String>>,
    pub time: Option<String>,
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path).map_err(|e| {
            GymSniperError::Config(format!("Failed to read config file '{}': {}", path, e))
        })?;

        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}
