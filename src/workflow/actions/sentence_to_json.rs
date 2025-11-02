use crate::json_helper::sanitize_json;
use crate::models::config::load_models_config;
use crate::models::providers::ModelProvider;
use crate::models::EnhancedEndpoint;
use crate::prompts::PromptManager;
use std::{error::Error, sync::Arc};
use crate::app_log;

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
    app_log!(debug, 
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

// New structured parameter extraction function using v2 prompt
pub async fn sentence_to_json_structured(
    sentence: &str,
    endpoint: &EnhancedEndpoint,
    provider: Arc<dyn ModelProvider>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    app_log!(info, 
        "Starting structured parameter extraction for endpoint: {}",
        endpoint.id
    );

    let prompt_manager = PromptManager::new().await?;

    // Prepare parameter lists for the prompt
    let required_params: Vec<String> = endpoint
        .parameters
        .iter()
        .filter(|p| p.required.unwrap_or(false))
        .map(|p| format!("{}: {}", p.name, p.description))
        .collect();

    let optional_params: Vec<String> = endpoint
        .parameters
        .iter()
        .filter(|p| !p.required.unwrap_or(false))
        .map(|p| format!("{}: {}", p.name, p.description))
        .collect();

    let required_params_str = if required_params.is_empty() {
        "None".to_string()
    } else {
        required_params.join("\n")
    };

    let optional_params_str = if optional_params.is_empty() {
        "None".to_string()
    } else {
        optional_params.join("\n")
    };

    // Generate structured prompt using v2 template
    let full_prompt = prompt_manager.format_sentence_to_json_v2(
        sentence,
        &endpoint.description,
        &required_params_str,
        &optional_params_str,
        Some("v2"),
    );

    app_log!(debug, "Generated structured prompt:\n{}", full_prompt);

    // Load model configuration
    let models_config = load_models_config().await?;
    let model_config = &models_config.default;

    let result = provider.generate(&full_prompt, model_config).await?;
    app_log!(debug, 
        "Raw LLM response for structured extraction:\n{:?}",
        result.content
    );

    // Parse the JSON response
    let parsed_json = sanitize_json(&result.content)?;
    app_log!(debug, "Parsed JSON: {:?}", parsed_json);

    // Validate that we got an object (not the old endpoints array format)
    if !parsed_json.is_object() {
        app_log!(error, "Invalid JSON structure: expected object with parameter values");
        return Err("Invalid JSON structure: expected object with parameter values".into());
    }

    // Additional validation: ensure only known parameters are present
    if let Some(obj) = parsed_json.as_object() {
        let known_param_names: Vec<&str> = endpoint
            .parameters
            .iter()
            .map(|p| p.name.as_str())
            .collect();

        for key in obj.keys() {
            if !known_param_names.contains(&key.as_str()) {
                app_log!(debug, 
                    "Warning: LLM returned unknown parameter '{}', ignoring",
                    key
                );
            }
        }

        // Filter out unknown parameters
        let mut filtered_obj = serde_json::Map::new();
        for (key, value) in obj {
            if known_param_names.contains(&key.as_str()) {
                filtered_obj.insert(key.clone(), value.clone());
            }
        }

        let filtered_json = serde_json::Value::Object(filtered_obj.clone());
        app_log!(info, 
            "Successfully extracted and validated {} parameters",
            filtered_obj.len()
        );
        return Ok(filtered_json);
    }

    app_log!(info, "Successfully completed structured parameter extraction");
    Ok(parsed_json)
}
