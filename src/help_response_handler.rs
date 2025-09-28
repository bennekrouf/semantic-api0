// src/help_response_handler.rs - Using prompts.yaml with minimal transformation
use crate::models::config::load_models_config;
use crate::models::providers::{GenerationResult, ModelProvider};
use crate::models::EnhancedEndpoint;
use crate::prompts::PromptManager;
use std::error::Error;
use std::sync::Arc;
use tracing::{debug, info};

pub async fn handle_help_request(
    sentence: &str,
    available_endpoints: &[EnhancedEndpoint],
    provider: Arc<dyn ModelProvider>,
) -> Result<GenerationResult, Box<dyn Error + Send + Sync>> {
    info!("Handling help request for: {}", sentence);

    // First, detect the language using LLM
    let detected_language = detect_language_with_llm(sentence, provider.clone()).await?;
    debug!("Detected language: {}", detected_language);

    // Create the exact endpoints list
    let endpoints_list = create_exact_endpoints_list(available_endpoints);
    debug!(
        "Generated exact endpoints list with {} endpoints",
        available_endpoints.len()
    );

    // If English, return direct response without LLM transformation
    if detected_language == "en" {
        let direct_response = format!(
            "Here are the available actions:\n\n{}\n\nYou can copy any of these descriptions to try them out.",
            endpoints_list
        );

        // Estimate token usage for the direct response
        let enhanced_calculator = crate::utils::token_calculator::EnhancedTokenCalculator::new();
        let usage = enhanced_calculator.calculate_usage(sentence, &direct_response, "direct");

        return Ok(GenerationResult {
            content: direct_response,
            usage,
        });
    }

    // For non-English, use the prompt from prompts.yaml
    let prompt_manager = PromptManager::new().await?;
    let full_prompt = prompt_manager.format_help_response_with_language(
        sentence,
        &endpoints_list,
        &detected_language,
        Some("v3"), // Use v3 for minimal transformation
    );

    debug!("Generated help prompt using prompts.yaml");

    let models_config = load_models_config().await?;
    let model_config = &models_config.default;

    let result = provider.generate(&full_prompt, model_config).await?;

    info!("Successfully generated help response");
    Ok(result)
}

async fn detect_language_with_llm(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let prompt_manager = PromptManager::new().await?;
    let language_detection_prompt = prompt_manager.language_detection(sentence, Some("v1"));

    let models_config = load_models_config().await?;
    let model_config = &models_config.default;

    let result = provider
        .generate(&language_detection_prompt, model_config)
        .await?;

    let detected_language = result.content.trim().to_lowercase();

    // Validate the response is a known language code
    let valid_languages = [
        "en", "fr", "es", "de", "it", "pt", "nl", "ru", "ja", "zh", "ko", "ar",
    ];
    if valid_languages.contains(&detected_language.as_str()) {
        debug!("LLM detected language: {}", detected_language);
        Ok(detected_language)
    } else {
        debug!(
            "LLM returned invalid language code '{}', defaulting to 'en'",
            detected_language
        );
        Ok("en".to_string())
    }
}

fn create_exact_endpoints_list(endpoints: &[EnhancedEndpoint]) -> String {
    if endpoints.is_empty() {
        return "No capabilities currently available.".to_string();
    }

    let mut capabilities: Vec<String> = Vec::new();

    // Output EXACT endpoint information without any modification
    for endpoint in endpoints {
        let mut endpoint_info = format!("• {}", endpoint.description);

        // Add example from endpoint.text if it differs meaningfully from description
        if !endpoint.text.is_empty() && endpoint.text != endpoint.description {
            endpoint_info.push_str(&format!("\n  Example: \"{}\"", endpoint.text));
        }

        capabilities.push(endpoint_info);
    }

    capabilities.join("\n\n")
}

