use crate::app_log;
use crate::json_helper::sanitize_json;
use crate::models::config::load_models_config;
use crate::models::providers::ModelProvider;
use crate::prompts::PromptManager;
use std::{error::Error, sync::Arc};

pub async fn sentence_to_json(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let prompt_manager = PromptManager::new().await?;
    let full_prompt = prompt_manager.format_sentence_to_json(sentence, Some("v1"));

    let models_config = load_models_config().await?;
    let model_config = &models_config.default;

    let result = provider.generate(&full_prompt, model_config).await?;

    // Log token usage
    app_log!(
        debug,
        provider = provider.get_model_name(),
        estimated = result.usage.estimated,
        input_tokens = result.usage.input_tokens,
        output_tokens = result.usage.output_tokens,
        total_tokens = result.usage.total_tokens,
        "sentence_to_json LLM request completed"
    );

    app_log!(debug, "Raw LLM response:\n{}", result.content);

    let parsed_json = sanitize_json(&result.content)?;

    // Validate the JSON structure for v1 format
    if !parsed_json.is_object() || parsed_json.get("endpoints").is_none() {
        app_log!(error, "Invalid JSON structure: missing 'endpoints' array");
        return Err("Invalid JSON structure: missing 'endpoints' array".into());
    }

    let endpoints = parsed_json
        .get("endpoints")
        .and_then(|e| e.as_array())
        .ok_or_else(|| {
            app_log!(error, "Invalid JSON structure: 'endpoints' is not an array");
            "Invalid JSON structure: 'endpoints' is not an array"
        })?;

    if endpoints.is_empty() {
        app_log!(error, "Invalid JSON structure: 'endpoints' array is empty");
        return Err("Invalid JSON structure: 'endpoints' array is empty".into());
    }

    app_log!(info, "Successfully generated and validated JSON");
    Ok(parsed_json)
}
