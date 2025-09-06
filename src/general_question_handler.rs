// src/general_question_handler.rs
use crate::models::config::load_models_config;
use crate::models::providers::ModelProvider;
use std::error::Error;
use std::sync::Arc;

pub async fn handle_general_question(
    question: &str,
    provider: Arc<dyn ModelProvider>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let prompt = format!(
        "You are a helpful assistant. Answer this question naturally and conversationally: {}",
        question
    );

    let models_config = load_models_config().await?;
    let model_config = &models_config.sentence_to_json; // Reuse existing config

    provider.generate(&prompt, model_config).await
}
