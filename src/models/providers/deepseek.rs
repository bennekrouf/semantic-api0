// src/models/providers/deepseek.rs
use super::{GenerationResult, ModelConfig, ModelProvider, ProviderConfig, TokenCounter};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tracing::{debug, error, info};

pub struct DeepSeekProvider {
    api_key: String,
    base_url: String,
}

#[derive(Serialize)]
struct DeepSeekRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f64,
    max_tokens: u32,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct DeepSeekResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

impl DeepSeekProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        if !config.enabled {
            debug!("Creating DeepSeek provider, but it's disabled in config");
        }

        Self {
            api_key: config
                .api_key
                .clone()
                .expect("DeepSeek API key not specified"),
            base_url: "https://api.deepseek.com/v1/chat/completions".to_string(),
        }
    }
}

#[async_trait]
impl ModelProvider for DeepSeekProvider {
    async fn generate(
        &self,
        prompt: &str,
        config: &ModelConfig,
    ) -> Result<GenerationResult, Box<dyn Error + Send + Sync>> {
        debug!("Generating response with DeepSeek API");

        let request = DeepSeekRequest {
            model: config.deepseek.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature: config.temperature as f64,
            max_tokens: config.max_tokens,
        };

        let client = reqwest::Client::new();
        let response = client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!(
                "DeepSeek request failed with status {}: {}",
                status, error_text
            );
            return Err(format!("DeepSeek request failed: {status} - {error_text}").into());
        }

        // Get raw JSON first for token extraction
        let response_json: serde_json::Value = response.json().await?;

        let deepseek_response: DeepSeekResponse = serde_json::from_value(response_json.clone())?;

        let content = deepseek_response
            .choices
            .first()
            .ok_or("No choices in DeepSeek response")?
            .message
            .content
            .clone();

        if content.trim().is_empty() {
            error!("Received empty response from DeepSeek");
            return Err("Empty response from DeepSeek".into());
        }

        let counter = TokenCounter::new();
        let usage = if let Some(usage_data) = deepseek_response.usage {
            debug!("DeepSeek actual token usage: {:?}", usage_data);
            crate::models::providers::token_counter::TokenUsage {
                input_tokens: usage_data.prompt_tokens,
                output_tokens: usage_data.completion_tokens,
                total_tokens: usage_data.total_tokens,
                estimated: false,
            }
        } else {
            debug!("No usage data from DeepSeek, using estimation");
            counter.from_response(&content, prompt, "deepseek")
        };

        debug!("DeepSeek final token usage: {:?}", usage);

        info!("Successfully received response from DeepSeek API");
        Ok(GenerationResult { content, usage })
    }

    fn get_model_name(&self) -> &str {
        "deepseek"
    }
}

