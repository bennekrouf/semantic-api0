use std::error::Error;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::models::config::load_models_config;
use crate::models::providers::ModelProvider;
use crate::models::ConfigFile;
use crate::models::Endpoint;
use crate::prompts::PromptManager;
use crate::workflow::extract_matched_action::extract_matched_action;
use crate::workflow::find_endpoint::find_endpoint_by_substring;

pub async fn find_closest_endpoint(
    config: &ConfigFile,
    input_sentence: &str,
    provider: Arc<dyn ModelProvider>,
) -> Result<Endpoint, Box<dyn Error + Send + Sync>> {
    info!("Starting endpoint matching for input: {}", input_sentence);
    debug!("Available endpoints: {}", config.endpoints.len());

    // Load model configuration
    let models_config = load_models_config().await?;
    let model_config = &models_config.find_endpoint;

    // Initialize the PromptManager
    let prompt_manager = PromptManager::new().await?;

    // Generate the actions list
    let actions_list = config
        .endpoints
        .iter()
        .map(|e| format!("- {}", e.text))
        .collect::<Vec<String>>()
        .join("\n");

    // Get formatted prompt from PromptManager
    let prompt = prompt_manager.format_find_endpoint(input_sentence, &actions_list, Some("v1"));
    debug!("Generated prompt:\n{}", prompt);

    // Use the provider with the Cohere model
    info!("Using provider with Cohere model: {}", model_config.cohere);
    let raw_response = provider.generate(&prompt, model_config).await?;
    debug!("Raw model response: '{}'", raw_response);

    let cleaned_response = extract_matched_action(&raw_response).await?;
    info!("Cleaned response: '{}'", cleaned_response);

    let matched_endpoint = match find_endpoint_by_substring(config, &cleaned_response) {
        Ok(endpoint) => endpoint.clone(),
        Err(_) => {
            error!("No endpoint matched the response: '{}'", cleaned_response);
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No matching endpoint found",
            )));
        }
    };

    info!("Found matching endpoint: {}", matched_endpoint.id);
    Ok(matched_endpoint)
}

