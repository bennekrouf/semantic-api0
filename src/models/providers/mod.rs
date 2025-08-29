// src/models/providers/mod.rs
use async_trait::async_trait;
use serde::Deserialize;
use std::error::Error;

pub mod claude;
pub mod cohere;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn generate(
        &self,
        prompt: &str,
        model: &ModelConfig,
    ) -> Result<String, Box<dyn Error + Send + Sync>>;
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ModelConfig {
    #[serde(default)]
    pub cohere: String,
    #[serde(default)]
    pub claude: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ModelsConfig {
    pub sentence_to_json: ModelConfig,
    pub find_endpoint: ModelConfig,
    pub semantic_match: ModelConfig,
}

pub fn create_provider(
    config: &ProviderConfig,
    provider_type: &str,
) -> Option<Box<dyn ModelProvider>> {
    if !config.enabled {
        return None;
    }

    if config.api_key.is_some() {
        match provider_type {
            "cohere" => Some(Box::new(cohere::CohereProvider::new(config))),
            "claude" => Some(Box::new(claude::ClaudeProvider::new(config))),
            _ => None,
        }
    } else {
        None
    }
}
