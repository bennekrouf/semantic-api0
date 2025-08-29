// src/models/providers/claude.rs
use super::{ModelConfig, ModelProvider, ProviderConfig};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tracing::{debug, error, info};

pub struct ClaudeProvider {
    api_key: String,
}

#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    temperature: f64,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentItem>,
}

#[derive(Debug, Deserialize)]
struct ContentItem {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

impl ClaudeProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        if !config.enabled {
            debug!("Creating Claude provider, but it's disabled in config");
        }

        Self {
            api_key: config
                .api_key
                .clone()
                .expect("Claude API key not specified"),
        }
    }
}

#[async_trait]
impl ModelProvider for ClaudeProvider {
    async fn generate(
        &self,
        prompt: &str,
        config: &ModelConfig,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        debug!("Generating response with Claude API");

        let request = ClaudeRequest {
            model: config.claude.clone(),
            max_tokens: config.max_tokens,
            temperature: config.temperature as f64,
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        let client = reqwest::Client::new();
        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!(
                "Claude request failed with status {}: {}",
                status, error_text
            );
            return Err(format!("Claude request failed: {} - {}", status, error_text).into());
        }

        let response_json: ClaudeResponse = response.json().await?;

        let content = response_json
            .content
            .first()
            .ok_or("No content in Claude response")?
            .text
            .clone();

        if content.trim().is_empty() {
            error!("Received empty response from Claude");
            return Err("Empty response from Claude".into());
        }

        info!("Successfully received response from Claude API");
        Ok(content)
    }
}

