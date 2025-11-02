use crate::models::config::load_models_config;
use crate::models::providers::ModelProvider;
use crate::prompts::PromptManager;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::Arc;
use crate::app_log;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum IntentType {
    ActionableRequest,
    GeneralQuestion,
    HelpRequest,
}

pub async fn classify_intent(
    sentence: &str,
    available_endpoints: &[String],
    provider: Arc<dyn ModelProvider>,
) -> Result<IntentType, Box<dyn Error + Send + Sync>> {
    app_log!(info, "Classifying intent for: {}", sentence);

    let prompt_manager = PromptManager::new().await?;
    let endpoints_list = available_endpoints.join("\n- ");

    // Use v3 prompt that supports HELP classification
    let prompt = prompt_manager.format_intent_classification(sentence, &endpoints_list, Some("v3"));
    app_log!(debug, "Generated intent classification prompt: {}", prompt);

    let models_config = load_models_config().await?;
    let model_config = &models_config.default;

    let response = provider.generate(&prompt, model_config).await?;
    app_log!(debug, "Intent classification response: {:?}", response);

    // Direct keyword extraction - search entire response
    let response_upper = response.content.to_uppercase();

    if response_upper.contains("ACTIONABLE") {
        app_log!(info, "Found 'ACTIONABLE' - classified as actionable request");
        Ok(IntentType::ActionableRequest)
    } else if response_upper.contains("HELP") {
        app_log!(info, "Found 'HELP' - classified as help request");
        Ok(IntentType::HelpRequest)
    } else if response_upper.contains("GENERAL") {
        app_log!(info, "Found 'GENERAL' - classified as general question");
        Ok(IntentType::GeneralQuestion)
    } else {
        // Enhanced fallback logic for better classification
        let sentence_lower = sentence.to_lowercase();

        // Check for help-related keywords
        let help_keywords = [
            "what can i do",
            "que puis-je faire",
            "qu'est-ce que je peux faire avec cette application",
            "what can i do with this app",
            "help",
            "aide",
            "aidez-moi",
            "available",
            "disponible",
            "options",
            "capabilities",
            "capacités",
            "features",
            "fonctionnalités",
            "how to use",
            "comment utiliser",
            "show me",
            "montre-moi",
            "list",
            "lister",
            "was kann ich",
            "hilfe",
            "wie kann",
            "fähigkeiten",
            "qué puedo",
            "ayuda",
            "ayúdame",
            "cómo",
            "capacidades",
        ];

        if help_keywords
            .iter()
            .any(|keyword| sentence_lower.contains(keyword))
        {
            app_log!(info, "Fallback: detected help keywords, classifying as help request");
            Ok(IntentType::HelpRequest)
        } else {
            // Default to general if no clear classification
            app_log!(info, "No clear classification found, defaulting to general question");
            Ok(IntentType::GeneralQuestion)
        }
    }
}
