use std::error::Error;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::models::config::load_models_config;
use crate::models::providers::ModelProvider;
use crate::models::{Endpoint, EnhancedEndpoint};
use crate::prompts::PromptManager;

pub async fn find_closest_endpoint_pure_llm(
    enhanced_endpoints: &[EnhancedEndpoint],
    input_sentence: &str,
    provider: Arc<dyn ModelProvider>,
) -> Result<EnhancedEndpoint, Box<dyn Error + Send + Sync>> {
    info!(
        "Starting pure LLM endpoint matching for input: {}",
        input_sentence
    );
    debug!("Available endpoints: {}", enhanced_endpoints.len());

    if enhanced_endpoints.is_empty() {
        return Err("No endpoints available for matching".into());
    }

    // Load model configuration
    let models_config = load_models_config().await?;
    let model_config = &models_config.find_endpoint;

    // Initialize the PromptManager
    let prompt_manager = PromptManager::new().await?;

    // Create structured endpoints list for the prompt
    let mut endpoints_list = String::new();
    for (_index, endpoint) in enhanced_endpoints.iter().enumerate() {
        endpoints_list.push_str(&format!(
            "- {} ({})\n", // Remove numbering, use bullet points
            endpoint.id, endpoint.description
        ));
    }

    // Get formatted prompt from PromptManager using v2
    let prompt =
        prompt_manager.format_find_endpoint_v2(input_sentence, &endpoints_list, Some("v2"));
    debug!("Generated prompt:\n{}", prompt);

    // Use the provider to get LLM response
    info!("Using LLM for semantic endpoint selection");
    let raw_response = provider.generate(&prompt, model_config).await?;
    debug!("Raw LLM response: '{:?}'", raw_response);

    // Extract endpoint ID from response
    let endpoint_id = raw_response.content.trim();

    if endpoint_id == "NO_MATCH" {
        error!("LLM determined no suitable endpoint matches the input");
        return Err("No suitable endpoint found for the given input".into());
    }

    // Find the matching endpoint by ID
    let matched_endpoint = enhanced_endpoints
        .iter()
        .find(|e| e.id == endpoint_id)
        .cloned();

    match matched_endpoint {
        Some(endpoint) => {
            info!("Successfully matched endpoint: {}", endpoint.id);
            Ok(endpoint)
        }
        None => {
            warn!(
                "LLM returned endpoint ID '{}' which doesn't exist in available endpoints",
                endpoint_id
            );
            error!(
                "Available endpoint IDs: {:?}",
                enhanced_endpoints.iter().map(|e| &e.id).collect::<Vec<_>>()
            );

            // Fallback: try partial matching in case of minor formatting issues
            let fallback_match = enhanced_endpoints
                .iter()
                .find(|e| {
                    e.id.to_lowercase().contains(&endpoint_id.to_lowercase())
                        || endpoint_id.to_lowercase().contains(&e.id.to_lowercase())
                })
                .cloned();

            match fallback_match {
                Some(endpoint) => {
                    warn!("Found fallback match: {}", endpoint.id);
                    Ok(endpoint)
                }
                None => Err(format!(
                    "Endpoint ID '{}' not found in available endpoints. Available IDs: [{}]",
                    endpoint_id,
                    enhanced_endpoints
                        .iter()
                        .map(|e| e.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
                .into()),
            }
        }
    }
}

// Keep the old function for backward compatibility during transition
pub async fn find_closest_endpoint(
    config: &crate::models::ConfigFile,
    input_sentence: &str,
    provider: Arc<dyn ModelProvider>,
) -> Result<Endpoint, Box<dyn Error + Send + Sync>> {
    // Convert ConfigFile endpoints to EnhancedEndpoint format for the new function
    let enhanced_endpoints: Vec<EnhancedEndpoint> = config
        .endpoints
        .iter()
        .map(|e| EnhancedEndpoint {
            id: e.id.clone(),
            name: e.text.clone(),
            text: e.text.clone(),
            description: e.description.clone(),
            verb: "POST".to_string(), // Default values
            base: "".to_string(),
            path: format!("/{}", e.id),
            essential_path: format!("/{}", e.id),
            api_group_id: "default".to_string(),
            api_group_name: "Default Group".to_string(),
            parameters: e.parameters.clone(),
        })
        .collect();

    let enhanced_result =
        find_closest_endpoint_pure_llm(&enhanced_endpoints, input_sentence, provider).await?;

    // Convert back to regular Endpoint
    Ok(Endpoint {
        id: enhanced_result.id,
        text: enhanced_result.text,
        description: enhanced_result.description,
        parameters: enhanced_result.parameters,
    })
}
