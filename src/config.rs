//! Daemon configuration, entirely from environment variables.

use std::env;
use std::path::PathBuf;

pub struct Config {
    /// OSTORE_ROOT (required): bound root directory.
    pub root: PathBuf,
    /// OSTORE_TOKEN (required): bearer token.
    pub token: String,
    /// OSTORE_HOST (default 127.0.0.1).
    pub host: String,
    /// OSTORE_PORT (default 8781).
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let root = env::var("OSTORE_ROOT")
            .map_err(|_| "OSTORE_ROOT is required".to_string())?;
        if root.is_empty() {
            return Err("OSTORE_ROOT must not be empty".to_string());
        }
        let token = env::var("OSTORE_TOKEN")
            .map_err(|_| "OSTORE_TOKEN is required".to_string())?;
        if token.is_empty() {
            return Err("OSTORE_TOKEN must not be empty".to_string());
        }
        let host = env::var("OSTORE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = match env::var("OSTORE_PORT") {
            Ok(p) => p
                .parse::<u16>()
                .map_err(|_| format!("OSTORE_PORT is not a valid port: {p}"))?,
            Err(_) => 8781,
        };
        Ok(Self { root: PathBuf::from(root), token, host, port })
    }
}
