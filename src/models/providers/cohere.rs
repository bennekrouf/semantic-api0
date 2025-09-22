// src/models/providers/cohere.rs - Fix token extraction
use super::{GenerationResult, ModelConfig, ModelProvider, ProviderConfig, TokenCounter};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tracing::{debug, error, warn};

pub struct CohereProvider {
    api_key: String,
}

#[derive(Serialize)]
struct CohereRequest {
    model: String,
    message: String,
    temperature: f64,
    max_tokens: u32,
    #[serde(rename = "chat_history")]
    chat_history: Vec<ChatMessage>,
    #[serde(rename = "response_format")]
    response_format: Option<ResponseFormat>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    message: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(Debug, Deserialize)]
struct CohereResponse {
    text: String,
    meta: Option<CohereMeta>,
}

#[derive(Debug, Deserialize)]
struct CohereMeta {
    tokens: Option<CohereTokens>,
}

#[derive(Debug, Deserialize)]
struct CohereTokens {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

impl CohereProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        if !config.enabled {
            debug!("Creating Cohere provider, but it's disabled in config");
        }

        Self {
            api_key: config
                .api_key
                .clone()
                .expect("Cohere API key not specified"),
        }
    }
}

#[async_trait]
impl ModelProvider for CohereProvider {
    async fn generate(
        &self,
        prompt: &str,
        config: &ModelConfig,
    ) -> Result<GenerationResult, Box<dyn Error + Send + Sync>> {
        debug!("Generating response with Cohere API");

        let request = CohereRequest {
            model: config.cohere.clone(),
            message: prompt.to_string(),
            temperature: config.temperature as f64,
            max_tokens: config.max_tokens,
            chat_history: vec![],
            response_format: None,
        };

        let client = reqwest::Client::new();
        let response = client
            .post("https://api.cohere.ai/v1/chat")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!(
                "Cohere request failed with status {}: {}",
                status, error_text
            );
            return Err(format!("Cohere request failed: {} - {}", status, error_text).into());
        }

        // Get raw JSON first for token extraction
        let response_json: serde_json::Value = response.json().await?;
        debug!("Cohere raw response: {:?}", response_json);

        let content = response_json["text"]
            .as_str()
            .ok_or("No text in Cohere response")?
            .to_string();

        if content.trim().is_empty() {
            error!("Received empty response from Cohere");
            return Err("Empty response from Cohere".into());
        }

        let counter = TokenCounter::new();

        // Try to extract actual token usage from Cohere response
        let usage = if let Some(meta) = response_json.get("meta") {
            if let Some(tokens) = meta.get("tokens") {
                let input_tokens = tokens
                    .get("input_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or_else(|| counter.estimate_tokens(prompt, "cohere") as u64)
                    as u32;

                let output_tokens = tokens
                    .get("output_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or_else(|| counter.estimate_tokens(&content, "cohere") as u64)
                    as u32;

                crate::models::providers::token_counter::TokenUsage {
                    input_tokens,
                    output_tokens,
                    total_tokens: input_tokens + output_tokens,
                    estimated: tokens.get("input_tokens").is_none()
                        || tokens.get("output_tokens").is_none(),
                }
            } else {
                warn!("No token information in Cohere meta, using estimation");
                counter.from_response(&content, prompt, "cohere")
            }
        } else {
            warn!("No meta field in Cohere response, using estimation");
            counter.from_response(&content, prompt, "cohere")
        };

        debug!("Cohere token usage: {:?}", usage);

        Ok(GenerationResult { content, usage })
    }

    fn get_model_name(&self) -> &str {
        "cohere"
    }
}
