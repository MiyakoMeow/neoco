//! Configuration module for loading neoco.toml

use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

use thiserror::Error;

type Result<T> = std::result::Result<T, ConfigError>;

/// Errors that can occur when loading or parsing configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Failed to read the config file from disk.
    #[error("Failed to read config file: {0}")]
    ReadFile(#[from] std::io::Error),

    /// Failed to parse the config file as TOML.
    #[error("Failed to parse config file: {0}")]
    ParseToml(#[from] toml::de::Error),

    /// The required API key environment variable is not set.
    #[error("Missing API key: set {0} environment variable")]
    MissingApiKey(String),
}

/// Provider type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderType {
    /// `OpenAI` Completions API (legacy)
    #[serde(rename = "openai")]
    OpenAICompletions,
    /// `OpenAI` Responses API (newer)
    OpenAIResponses,
    /// Anthropic API
    Anthropic,
}

/// Model provider configuration
#[derive(Debug, Deserialize, Clone)]
pub struct Provider {
    /// Provider type: openai, openai-responses, anthropic
    pub r#type: ProviderType,
    /// Display name
    pub name: String,
    /// API base URL
    pub base_url: String,
    /// Environment variable name for API key
    pub api_key_env: String,
}

/// Model group configuration
#[derive(Debug, Deserialize)]
struct ModelGroup {
    models: Vec<String>,
}

/// Full configuration from neoco.toml
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Default model (e.g., "minimax-cn/MiniMax-M2.5?temperature=0.1")
    pub model: Option<String>,
    /// Default model group
    pub model_group: Option<String>,
    /// Model groups: `group_name` -> vec![`model_with_provider`]
    #[serde(rename = "model_groups")]
    model_groups: HashMap<String, ModelGroup>,
    /// Model providers: `provider_name` -> Provider config
    #[serde(rename = "model_providers")]
    pub model_providers: HashMap<String, Provider>,
}

impl Config {
    /// Load config from default path .neoco/neoco.toml
    ///
    /// # Errors
    ///
    /// Returns an error if the config file cannot be read or parsed.
    pub fn load_default() -> Result<Self> {
        let path = Path::new(".neoco").join("neoco.toml");
        Self::load(path.as_path())
    }

    /// Load config from specified path
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed as valid TOML.
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        toml::from_str(&content).map_err(ConfigError::ParseToml)
    }

    /// Get model from model group
    #[must_use]
    pub fn get_model_from_group(&self, group: &str) -> Option<String> {
        self.model_groups
            .get(group)
            .and_then(|mg| mg.models.first().cloned())
    }

    /// Extract provider name from model string (e.g., "minimax-cn/MiniMax-M2.5" -> "minimax-cn")
    #[must_use]
    pub fn extract_provider(&self, model: &str) -> Option<&Provider> {
        model.split('/').next().and_then(|provider_name| {
            // Try exact match first
            self.model_providers.get(provider_name).or_else(|| {
                // Try case-insensitive match
                self.model_providers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(provider_name))
                    .map(|(_, v)| v)
            })
        })
    }

    /// Get API key from environment for the given provider
    ///
    /// # Errors
    ///
    /// Returns an error if the API key environment variable is not set.
    pub fn get_api_key(provider: &Provider) -> Result<String> {
        env::var(&provider.api_key_env)
            .map_err(|_| ConfigError::MissingApiKey(provider.api_key_env.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_provider() {
        let mut providers = HashMap::new();
        providers.insert(
            "minimax-cn".to_string(),
            Provider {
                r#type: ProviderType::OpenAICompletions,
                name: "MiniMax".to_string(),
                base_url: "https://api.minimaxi.com/v1".to_string(),
                api_key_env: "MINIMAX_API_KEY".to_string(),
            },
        );

        let config = Config {
            model: Some("minimax-cn/MiniMax-M2.5".to_string()),
            model_group: Some("balanced".to_string()),
            model_groups: HashMap::new(),
            model_providers: providers,
        };

        let provider = config.extract_provider("minimax-cn/MiniMax-M2.5");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name, "MiniMax");
    }
}
