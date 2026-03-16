//! Configuration module for loading neoco.toml

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

/// Provider type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderType {
    OpenAI,
    OpenAIResponses,
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

/// Full configuration from neoco.toml
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Default model (e.g., "minimax-cn/MiniMax-M2.5?temperature=0.1")
    pub model: Option<String>,
    /// Default model group
    pub model_group: Option<String>,
    /// Model groups: group_name -> vec![model_with_provider]
    #[serde(rename = "model_groups")]
    pub model_groups: HashMap<String, Vec<String>>,
    /// Model providers: provider_name -> Provider config
    #[serde(rename = "model_providers")]
    pub model_providers: HashMap<String, Provider>,
}

impl Config {
    /// Load config from default path .neoco/neoco.toml
    pub fn load_default() -> Result<Self> {
        let path = Path::new(".neoco").join("neoco.toml");
        Self::load(path.as_path())
    }

    /// Load config from specified path
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    }

    /// Get model from model group
    pub fn get_model_from_group(&self, group: &str) -> Option<String> {
        self.model_groups
            .get(group)
            .and_then(|models| models.first().cloned())
    }

    /// Extract provider name from model string (e.g., "minimax-cn/MiniMax-M2.5" -> "minimax-cn")
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
    pub fn get_api_key(&self, provider: &Provider) -> Result<String> {
        env::var(&provider.api_key_env).with_context(|| {
            format!(
                "Missing API key: set {} environment variable",
                provider.api_key_env
            )
        })
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
                r#type: ProviderType::OpenAI,
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
