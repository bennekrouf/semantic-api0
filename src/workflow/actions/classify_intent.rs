use crate::models::config::load_models_config;
use crate::models::providers::ModelProvider;
use crate::prompts::PromptManager;
use std::error::Error;
use std::sync::Arc;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub enum IntentType {
    ActionableRequest,
    GeneralQuestion,
}

pub async fn classify_intent(
    sentence: &str,
    available_endpoints: &[String],
    provider: Arc<dyn ModelProvider>,
) -> Result<IntentType, Box<dyn Error + Send + Sync>> {
    info!("Classifying intent for: {}", sentence);

    let prompt_manager = PromptManager::new().await?;
    let endpoints_list = available_endpoints.join("\n- ");

    let prompt = prompt_manager.format_intent_classification(sentence, &endpoints_list, Some("v1"));

    let models_config = load_models_config().await?;
    let model_config = &models_config.intent_classification;

    let response = provider.generate(&prompt, model_config).await?;
    debug!("Intent classification response: {:?}", response);

    // Direct keyword extraction - search entire response
    let response_upper = response.content.to_uppercase();

    if response_upper.contains("ACTIONABLE") {
        info!("Found 'ACTIONABLE' - classified as actionable request");
        Ok(IntentType::ActionableRequest)
    } else if response_upper.contains("GENERAL") {
        info!("Found 'GENERAL' - classified as general question");
        Ok(IntentType::GeneralQuestion)
    } else {
        // Default to general if neither keyword found
        info!("No clear classification found, defaulting to general question");
        Ok(IntentType::GeneralQuestion)
    }
}
