use crate::analysis::parameter_extraction::extract_parameters_from_followup;
use crate::analysis::response_builders::{
    create_complete_progressive_response, create_partial_progressive_response,
};
use crate::app_log;
use crate::endpoint_client::get_enhanced_endpoints;
use crate::models::providers::ModelProvider;
use crate::models::EnhancedAnalysisResult;
use crate::progressive_matching::{OngoingMatch, ProgressiveMatchingManager};
use std::error::Error;
use std::sync::Arc;

// Dedicated progressive matching handler
pub async fn handle_progressive_followup(
    sentence: &str,
    conversation_id: &str,
    ongoing_match: &OngoingMatch,
    provider: Arc<dyn ModelProvider>,
    progressive_manager: &ProgressiveMatchingManager,
    api_url: &str,
    email: &str,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    app_log!(
        info,
        "Processing progressive follow-up for endpoint: {}",
        ongoing_match.endpoint_id
    );

    // Get the endpoint definition to understand its parameters
    let enhanced_endpoints = get_enhanced_endpoints(api_url, email).await?;
    let endpoint = enhanced_endpoints
        .iter()
        .find(|e| e.id == ongoing_match.endpoint_id)
        .ok_or_else(|| format!("Endpoint {} not found", ongoing_match.endpoint_id))?;

    app_log!(
        info,
        "Found endpoint: {} with {} parameters",
        endpoint.name,
        endpoint.parameters.len()
    );

    // Extract new parameters from the follow-up message
    let new_parameters =
        extract_parameters_from_followup(sentence, provider.clone(), &endpoint.parameters).await?;

    app_log!(
        info,
        "Extracted {} new parameters from follow-up",
        new_parameters.len()
    );

    if new_parameters.is_empty() {
        return Err("No parameters could be extracted from the follow-up message".into());
    }

    // Update the progressive match with new parameters
    progressive_manager
        .update_match(
            conversation_id,
            &ongoing_match.endpoint_id,
            new_parameters.clone(),
        )
        .await?;

    // Check if we're now complete
    let required_param_names: Vec<String> = endpoint
        .parameters
        .iter()
        .filter(|p| p.required.unwrap_or(false))
        .map(|p| p.name.clone())
        .collect();

    let completion_result = progressive_manager
        .check_completion(
            conversation_id,
            &ongoing_match.endpoint_id,
            required_param_names,
            &endpoint.parameters,
        )
        .await?;

    app_log!(
        info,
        "Progressive matching completion: {}% complete, is_complete: {}",
        completion_result.completion_percentage,
        completion_result.is_complete
    );

    if completion_result.is_complete {
        // Clean up the progressive match
        progressive_manager
            .complete_match(conversation_id, &ongoing_match.endpoint_id)
            .await?;

        app_log!(info, "Progressive matching completed successfully");
        create_complete_progressive_response(
            endpoint,
            completion_result,
            &Some(conversation_id.to_string()),
        )
        .await
    } else {
        app_log!(
            info,
            "Progressive matching still incomplete, prompting for more parameters"
        );
        create_partial_progressive_response(
            endpoint,
            completion_result,
            &Some(conversation_id.to_string()),
        )
        .await
    }
}
