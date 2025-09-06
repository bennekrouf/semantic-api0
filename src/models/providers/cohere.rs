// src/models/providers/cohere.rs
use super::{ModelConfig, ModelProvider, ProviderConfig};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tracing::{debug, error, info};

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
    // text: String,
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
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
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

        let response_json: serde_json::Value = response.json().await?;

        // Cohere API response format may vary, adjust based on actual response structure
        let content = response_json["text"]
            .as_str()
            .or_else(|| response_json["message"].as_str())
            .or_else(|| {
                // Check if it's in a nested structure
                response_json["response"]["text"].as_str()
            })
            .ok_or("Invalid response format from Cohere API")?
            .to_string();

        if content.trim().is_empty() {
            error!("Received empty response from Cohere");
            return Err("Empty response from Cohere".into());
        }

        info!("Successfully received response from Cohere API");
        Ok(content)
    }
}
