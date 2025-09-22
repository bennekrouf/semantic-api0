// src/models/providers/mod.rs
use async_trait::async_trait;
use serde::Deserialize;
use std::error::Error;
use token_counter::{TokenCounter, TokenUsage};

pub mod claude;
pub mod cohere;
pub mod deepseek;
pub mod token_counter;

#[derive(Debug)]
pub struct GenerationResult {
    pub content: String,
    pub usage: TokenUsage,
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn generate(
        &self,
        prompt: &str,
        model: &ModelConfig,
    ) -> Result<GenerationResult, Box<dyn Error + Send + Sync>>;

    fn get_model_name(&self) -> &str;
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
    #[serde(default)]
    pub deepseek: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ModelsConfig {
    pub sentence_to_json: ModelConfig,
    pub find_endpoint: ModelConfig,
    pub semantic_match: ModelConfig,
    pub intent_classification: ModelConfig,
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
            "deepseek" => Some(Box::new(deepseek::DeepSeekProvider::new(config))),
            _ => None,
        }
    } else {
        None
    }
}

pub struct ProviderWithTokens<T> {
    inner: T,
    counter: TokenCounter,
    model_name: String,
}

impl<T> ProviderWithTokens<T> {
    pub fn new(inner: T, model_name: String) -> Self {
        Self {
            inner,
            counter: TokenCounter::new(),
            model_name,
        }
    }
}

