// src/helpers.rs
use crate::models::providers::{create_provider, ModelProvider, ProviderConfig};
use crate::models::{MatchingInfo, MatchingStatus, UsageInfo};
use graflog::app_log;
use std::env;
// use std::sync::Arc;

pub fn create_default_matching_info() -> MatchingInfo {
    MatchingInfo {
        status: MatchingStatus::Complete,
        total_required_fields: 0,
        mapped_required_fields: 0,
        total_optional_fields: 0,
        mapped_optional_fields: 0,
        completion_percentage: 100.0,
        missing_required_fields: vec![],
        missing_optional_fields: vec![],
    }
}

pub fn create_usage_info(input: u32, output: u32, model: String, estimated: bool) -> UsageInfo {
    UsageInfo {
        input_tokens: input,
        output_tokens: output,
        total_tokens: input + output,
        model,
        estimated,
    }
}

pub fn create_provider_with_key(provider_type: &str) -> Result<Box<dyn ModelProvider>, String> {
    match provider_type {
        "cohere" => match env::var("COHERE_API_KEY") {
            Ok(api_key) => {
                app_log!(info, "Using Cohere API");
                let config = ProviderConfig {
                    enabled: true,
                    api_key: Some(api_key),
                };
                create_provider(&config, "cohere")
                    .map_err(|e| format!("Failed to create Cohere provider: {}", e))
            }
            Err(_) => {
                app_log!(
                    error,
                    "{} API key not found. Please set {}_API_KEY environment variable",
                    provider_type.to_uppercase(),
                    provider_type.to_uppercase()
                );
                Err(format!("{} API key not found", provider_type))
            }
        },
        "claude" => match env::var("CLAUDE_API_KEY") {
            Ok(api_key) => {
                app_log!(info, "Using Claude API");
                let config = ProviderConfig {
                    enabled: true,
                    api_key: Some(api_key),
                };
                create_provider(&config, "claude")
                    .map_err(|e| format!("Failed to create Claude provider: {}", e))
            }
            Err(_) => {
                app_log!(
                    error,
                    "{} API key not found. Please set {}_API_KEY environment variable",
                    provider_type.to_uppercase(),
                    provider_type.to_uppercase()
                );
                Err(format!("{} API key not found", provider_type))
            }
        },
        "deepseek" => match env::var("DEEPSEEK_API_KEY") {
            Ok(api_key) => {
                app_log!(info, "Using DeepSeek API");
                let config = ProviderConfig {
                    enabled: true,
                    api_key: Some(api_key),
                };
                create_provider(&config, "deepseek")
                    .map_err(|e| format!("Failed to create DeepSeek provider: {}", e))
            }
            Err(_) => {
                app_log!(
                    error,
                    "{} API key not found. Please set {}_API_KEY environment variable",
                    provider_type.to_uppercase(),
                    provider_type.to_uppercase()
                );
                Err(format!("{} API key not found", provider_type))
            }
        },
        _ => {
            app_log!(
                error,
                "Invalid provider: {}. Use 'cohere', 'claude', or 'deepseek'",
                provider_type
            );
            Err(format!("Invalid provider: {}", provider_type))
        }
    }
}
