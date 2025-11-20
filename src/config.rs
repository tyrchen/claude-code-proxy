use crate::error::{ProxyError, Result};
use serde::Deserialize;
use std::env;

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub server: ServerConfig,
    pub provider: ProviderConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub workers: usize,
}

#[derive(Debug, Clone)]
pub enum ProviderConfig {
    Gemini(GeminiConfig),
    Kimi(KimiConfig),
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeminiConfig {
    pub api_key: String,
    pub endpoint: String,
    /// Optional: Override default model mapping (from ANTHROPIC_MODEL env var)
    pub default_model: Option<String>,
    /// Whether to prompt model to update todo list after tool execution
    #[serde(default = "default_auto_todo_prompt")]
    pub auto_todo_prompt: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KimiConfig {
    pub api_key: String,
    pub endpoint: String,
    pub model: String,
}

fn default_auto_todo_prompt() -> bool {
    false // Disabled by default - Gemini doesn't reliably respond to todo update prompts
}

impl ProxyConfig {
    /// Load configuration from environment variables for the specified provider
    /// Supports both GEMINI_API_KEY and ANTHROPIC_AUTH_TOKEN for compatibility with Claude Code
    pub fn from_env(provider_type: &str) -> Result<Self> {
        let listen_addr = env::var("CLAUDE_CODE_PROXY_LISTEN_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string());

        let workers = env::var("PROXY_WORKERS")
            .unwrap_or_else(|_| "4".to_string())
            .parse::<usize>()
            .map_err(|e| ProxyError::ConfigError(format!("Invalid workers value: {}", e)))?;

        let provider = match provider_type {
            "gemini" => {
                // Try ANTHROPIC_AUTH_TOKEN first (for Claude Code compatibility), then fall back to GEMINI_API_KEY
                let api_key = env::var("ANTHROPIC_AUTH_TOKEN")
                    .or_else(|_| env::var("GEMINI_API_KEY"))
                    .map_err(|_| {
                        ProxyError::ConfigError(
                            "Neither ANTHROPIC_AUTH_TOKEN nor GEMINI_API_KEY is set".to_string(),
                        )
                    })?;

                let endpoint = env::var("GEMINI_ENDPOINT")
                    .unwrap_or_else(|_| "generativelanguage.googleapis.com".to_string());

                // Support ANTHROPIC_MODEL for overriding default model mapping
                let default_model = env::var("ANTHROPIC_MODEL").ok();

                // Support AUTO_TODO_PROMPT for enabling/disabling todo update prompts
                let auto_todo_prompt = env::var("AUTO_TODO_PROMPT")
                    .ok()
                    .and_then(|v| v.parse::<bool>().ok())
                    .unwrap_or(true);

                ProviderConfig::Gemini(GeminiConfig {
                    api_key,
                    endpoint,
                    default_model,
                    auto_todo_prompt,
                })
            }
            "kimi" => {
                let api_key = env::var("ANTHROPIC_AUTH_TOKEN")
                    .or_else(|_| env::var("KIMI_API_KEY"))
                    .map_err(|_| {
                        ProxyError::ConfigError(
                            "Neither ANTHROPIC_AUTH_TOKEN nor KIMI_API_KEY is set".to_string(),
                        )
                    })?;

                let endpoint = env::var("KIMI_ENDPOINT")
                    .unwrap_or_else(|_| "https://api.moonshot.ai/anthropic".to_string());

                let model = env::var("ANTHROPIC_MODEL")
                    .unwrap_or_else(|_| "kimi-k2-thinking-turbo".to_string());

                ProviderConfig::Kimi(KimiConfig {
                    api_key,
                    endpoint,
                    model,
                })
            }
            _ => {
                return Err(ProxyError::ConfigError(format!(
                    "Unknown provider: {}. Supported: gemini, kimi",
                    provider_type
                )));
            }
        };

        Ok(ProxyConfig {
            server: ServerConfig {
                listen_addr,
                workers,
            },
            provider,
        })
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        match &self.provider {
            ProviderConfig::Gemini(config) => {
                if config.api_key.is_empty() {
                    return Err(ProxyError::ConfigError("API key is empty".to_string()));
                }
                if config.endpoint.is_empty() {
                    return Err(ProxyError::ConfigError("Endpoint is empty".to_string()));
                }
            }
            ProviderConfig::Kimi(config) => {
                if config.api_key.is_empty() {
                    return Err(ProxyError::ConfigError("API key is empty".to_string()));
                }
                if config.endpoint.is_empty() {
                    return Err(ProxyError::ConfigError("Endpoint is empty".to_string()));
                }
                if config.model.is_empty() {
                    return Err(ProxyError::ConfigError("Model is empty".to_string()));
                }
            }
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
    fn test_gemini_config_validation() {
        let valid_config = ProxyConfig {
            server: ServerConfig {
                listen_addr: "127.0.0.1:8080".to_string(),
                workers: 4,
            },
            provider: ProviderConfig::Gemini(GeminiConfig {
                api_key: "test-key".to_string(),
                endpoint: "test.googleapis.com".to_string(),
                default_model: None,
                auto_todo_prompt: true,
            }),
        };

        assert!(valid_config.validate().is_ok());

        let invalid_config = ProxyConfig {
            server: ServerConfig {
                listen_addr: "127.0.0.1:8080".to_string(),
                workers: 0,
            },
            provider: ProviderConfig::Gemini(GeminiConfig {
                api_key: "test-key".to_string(),
                endpoint: "test.googleapis.com".to_string(),
                default_model: None,
                auto_todo_prompt: true,
            }),
        };

        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_kimi_config_validation() {
        let valid_config = ProxyConfig {
            server: ServerConfig {
                listen_addr: "127.0.0.1:8080".to_string(),
                workers: 4,
            },
            provider: ProviderConfig::Kimi(KimiConfig {
                api_key: "test-key".to_string(),
                endpoint: "https://api.moonshot.ai/anthropic".to_string(),
                model: "kimi-k2-thinking-turbo".to_string(),
            }),
        };

        assert!(valid_config.validate().is_ok());
    }
}
