use crate::json_helper::sanitize_json;
use crate::models::config::load_models_config;
use crate::models::providers::ModelProvider;
use crate::models::EnhancedEndpoint;
use crate::prompts::PromptManager;
use std::{error::Error, sync::Arc};
use tracing::{debug, error, info};

// Original function for backward compatibility
pub async fn sentence_to_json(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let prompt_manager = PromptManager::new().await?;
    let full_prompt = prompt_manager.format_sentence_to_json(sentence, Some("v1"));

    // Load model configuration
    let models_config = load_models_config().await?;
    let model_config = &models_config.sentence_to_json;

    let full_response_text = provider.generate(&full_prompt, model_config).await?;
    debug!("Raw LLM response:\n{}", full_response_text);

    let parsed_json = sanitize_json(&full_response_text)?;

    // Validate the JSON structure for v1 format
    if !parsed_json.is_object() || !parsed_json.get("endpoints").is_some() {
        error!("Invalid JSON structure: missing 'endpoints' array");
        return Err("Invalid JSON structure: missing 'endpoints' array".into());
    }

    let endpoints = parsed_json
        .get("endpoints")
        .and_then(|e| e.as_array())
        .ok_or_else(|| {
            error!("Invalid JSON structure: 'endpoints' is not an array");
            "Invalid JSON structure: 'endpoints' is not an array"
        })?;

    if endpoints.is_empty() {
        error!("Invalid JSON structure: 'endpoints' array is empty");
        return Err("Invalid JSON structure: 'endpoints' array is empty".into());
    }

    info!("Successfully generated and validated JSON");
    Ok(parsed_json)
}

// New structured parameter extraction function using v2 prompt
pub async fn sentence_to_json_structured(
    sentence: &str,
    endpoint: &EnhancedEndpoint,
    provider: Arc<dyn ModelProvider>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    info!(
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

    debug!("Generated structured prompt:\n{}", full_prompt);

    // Load model configuration
    let models_config = load_models_config().await?;
    let model_config = &models_config.sentence_to_json;

    let full_response_text = provider.generate(&full_prompt, model_config).await?;
    debug!(
        "Raw LLM response for structured extraction:\n{}",
        full_response_text
    );

    // Parse the JSON response
    let parsed_json = sanitize_json(&full_response_text)?;
    debug!("Parsed JSON: {:?}", parsed_json);

    // Validate that we got an object (not the old endpoints array format)
    if !parsed_json.is_object() {
        error!("Invalid JSON structure: expected object with parameter values");
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
                debug!(
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
        info!(
            "Successfully extracted and validated {} parameters",
            filtered_obj.len()
        );
        return Ok(filtered_json);
    }

    info!("Successfully completed structured parameter extraction");
    Ok(parsed_json)
}

// Hybrid function that can work with both approaches
pub async fn extract_parameters_adaptive(
    sentence: &str,
    endpoint: Option<&EnhancedEndpoint>,
    provider: Arc<dyn ModelProvider>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    match endpoint {
        Some(ep) => {
            info!("Using structured extraction (v2) for endpoint: {}", ep.id);
            sentence_to_json_structured(sentence, ep, provider).await
        }
        None => {
            info!("Using general extraction (v1) - no specific endpoint");
            sentence_to_json(sentence, provider).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EndpointParameter;

    fn create_test_endpoint() -> EnhancedEndpoint {
        EnhancedEndpoint {
            id: "send_email".to_string(),
            name: "Send Email".to_string(),
            text: "send email".to_string(),
            description: "Send an email message".to_string(),
            verb: "POST".to_string(),
            base: "https://api.example.com".to_string(),
            path: "/email/send".to_string(),
            essential_path: "/email/send".to_string(),
            api_group_id: "email".to_string(),
            api_group_name: "Email API".to_string(),
            parameters: vec![
                EndpointParameter {
                    name: "to".to_string(),
                    description: "Recipient email address".to_string(),
                    required: Some(true),
                    alternatives: Some(vec!["recipient".to_string(), "email_to".to_string()]),
                    semantic_value: None,
                },
                EndpointParameter {
                    name: "subject".to_string(),
                    description: "Email subject".to_string(),
                    required: Some(true),
                    alternatives: Some(vec!["title".to_string()]),
                    semantic_value: None,
                },
                EndpointParameter {
                    name: "body".to_string(),
                    description: "Email content".to_string(),
                    required: Some(false),
                    alternatives: Some(vec!["content".to_string(), "message".to_string()]),
                    semantic_value: None,
                },
            ],
        }
    }

    #[tokio::test]
    async fn test_structured_extraction_parameters() {
        let endpoint = create_test_endpoint();
        let sentence = "Send email to john@example.com with subject 'Meeting Tomorrow' and tell him we need to reschedule";

        // This test would need a mock provider to run
        // In real usage, it should extract:
        // {
        //   "to": "john@example.com",
        //   "subject": "Meeting Tomorrow",
        //   "body": "we need to reschedule"
        // }

        println!("Test endpoint: {:?}", endpoint);
        println!("Test sentence: {}", sentence);

        // Verify parameter structure
        let required_count = endpoint
            .parameters
            .iter()
            .filter(|p| p.required.unwrap_or(false))
            .count();
        let optional_count = endpoint
            .parameters
            .iter()
            .filter(|p| !p.required.unwrap_or(false))
            .count();

        assert_eq!(required_count, 2); // to, subject
        assert_eq!(optional_count, 1); // body
    }
}
