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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
[gym]
base_url = "https://example.com/clientportal2"
club_id = 42

[credentials]
email = "user@example.com"
password = "secret"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.gym.club_id, 42);
        assert_eq!(config.credentials.email, "user@example.com");
        assert!(config.targets.is_empty());
        assert!(config.email.is_none());
    }

    #[test]
    fn parse_full_config() {
        let toml_str = r#"
[gym]
base_url = "https://example.com/clientportal2"
club_id = 42

[credentials]
email = "user@example.com"
password = "secret"

[[targets]]
class_name = "Yoga"
days = ["monday", "wed"]
time = "09:00"

[[targets]]
class_name = "Spin"

[email]
smtp_server = "smtp.example.com"
smtp_port = 587
username = "user"
password = "pass"
from = "a@b.com"
to = "c@d.com"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.targets.len(), 2);
        assert_eq!(config.targets[0].class_name, "Yoga");
        assert_eq!(config.targets[0].days.as_ref().unwrap().len(), 2);
        assert_eq!(config.targets[1].time, None);
        assert!(config.email.is_some());
        assert_eq!(config.email.unwrap().smtp_port, 587);
    }

    #[test]
    fn parse_missing_required_fields() {
        let toml_str = r#"
[gym]
base_url = "https://example.com"
"#;
        let result: std::result::Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }
}
