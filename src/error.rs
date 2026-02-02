use thiserror::Error;

#[derive(Error, Debug)]
pub enum GymSniperError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, GymSniperError>;
