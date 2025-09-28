// src/models/providers/cohere.rs - Fix token extraction
use super::{GenerationResult, ModelConfig, ModelProvider, ProviderConfig};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tracing::{debug, error};

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
            return Err(format!("Cohere request failed: {status} - {error_text}").into());
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

        // let counter = TokenCounter::new();

        // Try to extract actual token usage from Cohere response
        let usage = if let Some(meta) = response_json.get("meta") {
            debug!("Found meta field in Cohere response");

            // Try new format first: meta.billed_units
            if let Some(billed_units) = meta.get("billed_units") {
                let input_tokens = billed_units
                    .get("input_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as u32;
                let output_tokens = billed_units
                    .get("output_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as u32;

                debug!(
                    "Cohere billed_units: input={}, output={}",
                    input_tokens, output_tokens
                );

                // Check if we got actual non-zero tokens
                if input_tokens > 0 || output_tokens > 0 {
                    crate::models::providers::token_counter::TokenUsage {
                        input_tokens,
                        output_tokens,
                        total_tokens: input_tokens + output_tokens,
                        estimated: false,
                    }
                } else {
                    // API returned 0 tokens - use enhanced estimation
                    debug!("Cohere returned 0 tokens, using enhanced estimation instead");
                    let enhanced_calculator =
                        crate::utils::token_calculator::EnhancedTokenCalculator::new();
                    enhanced_calculator.calculate_usage(prompt, &content, "cohere")
                }
            }
            // Try old format: meta.tokens
            else if let Some(tokens) = meta.get("tokens") {
                let input_tokens = tokens
                    .get("input_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as u32;
                let output_tokens = tokens
                    .get("output_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as u32;

                debug!(
                    "Cohere tokens: input={}, output={}",
                    input_tokens, output_tokens
                );

                // Check if we got actual non-zero tokens
                if input_tokens > 0 || output_tokens > 0 {
                    crate::models::providers::token_counter::TokenUsage {
                        input_tokens,
                        output_tokens,
                        total_tokens: input_tokens + output_tokens,
                        estimated: false,
                    }
                } else {
                    // API returned 0 tokens - use enhanced estimation
                    debug!("Cohere returned 0 tokens, using enhanced estimation instead");
                    let enhanced_calculator =
                        crate::utils::token_calculator::EnhancedTokenCalculator::new();
                    enhanced_calculator.calculate_usage(prompt, &content, "cohere")
                }
            } else {
                debug!("No token information in Cohere meta, using enhanced estimation");
                let enhanced_calculator =
                    crate::utils::token_calculator::EnhancedTokenCalculator::new();
                enhanced_calculator.calculate_usage(prompt, &content, "cohere")
            }
        } else {
            debug!("No meta field in Cohere response, using enhanced estimation");
            let enhanced_calculator =
                crate::utils::token_calculator::EnhancedTokenCalculator::new();
            enhanced_calculator.calculate_usage(prompt, &content, "cohere")
        };

        debug!("Cohere final token usage: {:?}", usage);

        debug!("Cohere token usage: {:?}", usage);

        Ok(GenerationResult { content, usage })
    }

    fn get_model_name(&self) -> &str {
        "cohere"
    }
}
