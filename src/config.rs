use crate::error::{ProxyError, Result};
use serde::Deserialize;
use std::env;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct ProxyConfig {
    pub server: ServerConfig,
    pub gemini: GeminiConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub workers: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeminiConfig {
    pub api_key: String,
    pub endpoint: String,
}

impl ProxyConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let listen_addr =
            env::var("PROXY_LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

        let workers = env::var("PROXY_WORKERS")
            .unwrap_or_else(|_| "4".to_string())
            .parse::<usize>()
            .map_err(|e| ProxyError::ConfigError(format!("Invalid workers value: {}", e)))?;

        let api_key = env::var("GEMINI_API_KEY")
            .map_err(|_| ProxyError::ConfigError("GEMINI_API_KEY not set".to_string()))?;

        let endpoint = env::var("GEMINI_ENDPOINT")
            .unwrap_or_else(|_| "generativelanguage.googleapis.com".to_string());

        Ok(ProxyConfig {
            server: ServerConfig {
                listen_addr,
                workers,
            },
            gemini: GeminiConfig { api_key, endpoint },
        })
    }

    /// Load configuration from TOML file
    pub fn from_file(path: &str) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .map_err(|e| ProxyError::ConfigError(format!("Failed to read config file: {}", e)))?;

        let mut config: ProxyConfig = toml::from_str(&contents)
            .map_err(|e| ProxyError::ConfigError(format!("Failed to parse config file: {}", e)))?;

        // Allow environment variables to override file config
        if let Ok(api_key) = env::var("GEMINI_API_KEY") {
            config.gemini.api_key = api_key;
        }

        Ok(config)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.gemini.api_key.is_empty() {
            return Err(ProxyError::ConfigError("API key is empty".to_string()));
        }

        if self.gemini.endpoint.is_empty() {
            return Err(ProxyError::ConfigError("Endpoint is empty".to_string()));
        }

        if self.server.workers == 0 {
            return Err(ProxyError::ConfigError(
                "Workers must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let valid_config = ProxyConfig {
            server: ServerConfig {
                listen_addr: "127.0.0.1:8080".to_string(),
                workers: 4,
            },
            gemini: GeminiConfig {
                api_key: "test-key".to_string(),
                endpoint: "test.googleapis.com".to_string(),
            },
        };

        assert!(valid_config.validate().is_ok());

        let invalid_config = ProxyConfig {
            server: ServerConfig {
                listen_addr: "127.0.0.1:8080".to_string(),
                workers: 0,
            },
            gemini: GeminiConfig {
                api_key: "test-key".to_string(),
                endpoint: "test.googleapis.com".to_string(),
            },
        };

        assert!(invalid_config.validate().is_err());
    }
}
