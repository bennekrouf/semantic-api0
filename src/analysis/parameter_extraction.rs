use crate::app_log;
use crate::json_helper::sanitize_json;
use crate::models::config::load_models_config;
use crate::models::providers::ModelProvider;
use crate::models::EndpointParameter;
use crate::progressive_matching::ParameterValue;
use crate::prompts::PromptManager;
use std::sync::Arc;

// Extract parameters from follow-up using the existing function from sentence_analysis.rs
pub async fn extract_parameters_from_followup(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    endpoint_parameters: &[EndpointParameter],
) -> Result<Vec<ParameterValue>, Box<dyn std::error::Error + Send + Sync>> {
    app_log!(info, "Extracting parameters from follow-up: '{}'", sentence);

    let prompt_manager = PromptManager::new().await?;
    let available_params: Vec<String> = endpoint_parameters
        .iter()
        .map(|p| format!("{}: {}", p.name, p.description))
        .collect();
    let available_params_str = available_params.join("\n");

    let prompt = prompt_manager.format_extract_followup_parameters_with_mapping(
        sentence,
        &available_params_str,
        Some("v1"),
    )?;

    let models_config = load_models_config().await?;
    let model_config = &models_config.default;

    let result = provider.generate(&prompt, model_config).await?;
    let json_result = sanitize_json(&result.content)?;

    let mut parameters = Vec::new();
    let valid_param_names: Vec<&str> = endpoint_parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();

    if let Some(obj) = json_result.as_object() {
        for (key, value) in obj {
            if let Some(str_value) = value.as_str() {
                if !str_value.trim().is_empty() && valid_param_names.contains(&key.as_str()) {
                    parameters.push(ParameterValue {
                        name: key.clone(),
                        value: str_value.trim().to_string(),
                        description: format!("User provided value for {key}"),
                    });
                }
            }
        }
    }

    Ok(parameters)
}
