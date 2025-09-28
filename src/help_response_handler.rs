// src/help_response_handler.rs
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

    // Create a human-readable list of capabilities from endpoints
    let endpoints_list = create_capabilities_list(available_endpoints);
    debug!("Generated capabilities list: {}", endpoints_list);

    let prompt_manager = PromptManager::new().await?;
    let full_prompt = prompt_manager.format_help_response_with_language(
        sentence,
        &endpoints_list,
        &detected_language,
        Some("v1"),
    );

    debug!("Generated help prompt: {}", full_prompt);

    let models_config = load_models_config().await?;
    let model_config = &models_config.default; // Reuse existing config

    let result = provider.generate(&full_prompt, model_config).await?;

    info!("Successfully generated help response");
    Ok(result)
}

async fn detect_language_with_llm(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let language_detection_prompt = format!(
        r#"Detect the language of this user input: "{sentence}"

Respond with ONLY the two-letter language code:
- en (English)
- fr (French) 
- es (Spanish)
- de (German)
- it (Italian)
- pt (Portuguese)
- nl (Dutch)
- ru (Russian)
- ja (Japanese)
- zh (Chinese)
- ko (Korean)
- ar (Arabic)

If the language is not in this list or unclear, respond with "en".
Respond with only the two-letter code, nothing else."#
    );

    let models_config = load_models_config().await?;
    let model_config = &models_config.default; // Use lightweight config

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

fn create_capabilities_list(endpoints: &[EnhancedEndpoint]) -> String {
    if endpoints.is_empty() {
        return "No capabilities currently available.".to_string();
    }

    let mut capabilities: Vec<String> = Vec::new();

    // Group endpoints by category or use individual descriptions
    for endpoint in endpoints {
        let capability = match endpoint.id.as_str() {
            id if id.contains("email") => format!("• Send emails ({})", endpoint.description),
            id if id.contains("meeting") || id.contains("schedule") => {
                format!(
                    "• Schedule meetings and appointments ({})",
                    endpoint.description
                )
            }
            id if id.contains("ticket") || id.contains("support") => {
                format!("• Create support tickets ({})", endpoint.description)
            }
            id if id.contains("report") || id.contains("generate") => {
                format!(
                    "• Generate reports and documents ({})",
                    endpoint.description
                )
            }
            id if id.contains("deploy") => {
                format!("• Deploy applications ({})", endpoint.description)
            }
            id if id.contains("payment") || id.contains("pay") => {
                format!("• Process payments ({})", endpoint.description)
            }
            id if id.contains("backup") => {
                format!("• Backup databases ({})", endpoint.description)
            }
            id if id.contains("log") => {
                format!("• Analyze application logs ({})", endpoint.description)
            }
            _ => format!("• {} ({})", endpoint.name, endpoint.description),
        };
        capabilities.push(capability); // This line was missing!
    }

    // Remove duplicates and sort
    capabilities.sort();
    capabilities.dedup();

    capabilities.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EndpointParameter;

    #[test]
    fn test_create_capabilities_list() {
        let endpoints = vec![
            EnhancedEndpoint {
                id: "send_email".to_string(),
                name: "Send Email".to_string(),
                text: "send email".to_string(),
                description: "Send an email with attachments".to_string(),
                verb: "POST".to_string(),
                base: "api".to_string(),
                path: "/email/send".to_string(),
                essential_path: "/email/send".to_string(),
                api_group_id: "communication".to_string(),
                api_group_name: "Communication APIs".to_string(),
                parameters: vec![],
            },
            EnhancedEndpoint {
                id: "schedule_meeting".to_string(),
                name: "Schedule Meeting".to_string(),
                text: "schedule meeting".to_string(),
                description: "Schedule a meeting or appointment".to_string(),
                verb: "POST".to_string(),
                base: "api".to_string(),
                path: "/calendar/schedule".to_string(),
                essential_path: "/calendar/schedule".to_string(),
                api_group_id: "calendar".to_string(),
                api_group_name: "Calendar APIs".to_string(),
                parameters: vec![],
            },
        ];

        let result = create_capabilities_list(&endpoints);
        assert!(result.contains("Send emails"));
        assert!(result.contains("Schedule meetings"));
    }

    #[test]
    fn test_empty_endpoints() {
        let endpoints = vec![];
        let result = create_capabilities_list(&endpoints);
        assert_eq!(result, "No capabilities currently available.");
    }
}
