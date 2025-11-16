use crate::general_question_handler::handle_general_question;
use crate::help_response_handler::handle_help_request;
use crate::models::providers::ModelProvider;
use crate::models::{EnhancedAnalysisResult, EnhancedEndpoint};
use crate::models::{MatchingInfo, MatchingStatus, MissingField, ParameterMatch, UsageInfo};
use crate::progressive_matching::ProgressiveMatchResult;
use crate::utils::path_params::add_path_parameters_to_list;
use crate::workflow::classify_intent::IntentType;
use std::error::Error;
use std::sync::Arc;

// Helper functions for creating responses
pub async fn create_complete_progressive_response(
    endpoint: &EnhancedEndpoint,
    result: ProgressiveMatchResult,
    conversation_id: &Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let base_parameters: Vec<ParameterMatch> = result
        .matched_parameters
        .into_iter()
        .map(|param| ParameterMatch {
            name: param.name,
            description: param.description,
            value: Some(param.value),
        })
        .collect();

    let (parameters, all_endpoint_parameters) =
        add_path_parameters_to_list(endpoint, base_parameters)?;
    let matching_info = MatchingInfo::compute(&parameters, &all_endpoint_parameters);

    let usage_info = UsageInfo {
        input_tokens: 50,
        output_tokens: 20,
        total_tokens: 70,
        model: "progressive_matching".to_string(),
        estimated: true,
    };

    Ok(EnhancedAnalysisResult {
        endpoint_id: endpoint.id.clone(),
        endpoint_name: endpoint.name.clone(),
        endpoint_description: endpoint.description.clone(),
        verb: endpoint.verb.clone(),
        base: endpoint.base.clone(),
        path: endpoint.path.clone(),
        essential_path: endpoint.essential_path.clone(),
        api_group_id: endpoint.api_group_id.clone(),
        api_group_name: endpoint.api_group_name.clone(),
        parameters,
        raw_json: serde_json::json!({
            "type": "progressive_complete",
            "endpoint_id": endpoint.id,
            "status": "complete",
            "completion_percentage": 100.0
        }),
        conversation_id: conversation_id.clone(),
        matching_info,
        user_prompt: None,
        total_input_tokens: usage_info.input_tokens,
        total_output_tokens: usage_info.output_tokens,
        usage: usage_info,
        intent: IntentType::ActionableRequest,
    })
}

pub async fn create_partial_progressive_response(
    endpoint: &EnhancedEndpoint,
    result: ProgressiveMatchResult,
    conversation_id: &Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let base_parameters: Vec<ParameterMatch> = result
        .matched_parameters
        .into_iter()
        .map(|param| ParameterMatch {
            name: param.name,
            description: param.description,
            value: Some(param.value),
        })
        .collect();

    let (parameters, all_endpoint_parameters) =
        add_path_parameters_to_list(endpoint, base_parameters)?;

    let missing_fields: Vec<MissingField> = result
        .missing_parameters
        .iter()
        .map(|param| MissingField {
            name: param.clone(),
            description: format!("Missing required parameter: {param}"),
        })
        .collect();

    let matching_info = MatchingInfo {
        status: MatchingStatus::Partial,
        total_required_fields: all_endpoint_parameters.len(),
        mapped_required_fields: parameters.iter().filter(|p| p.value.is_some()).count(),
        total_optional_fields: 0,
        mapped_optional_fields: 0,
        completion_percentage: result.completion_percentage,
        missing_required_fields: missing_fields,
        missing_optional_fields: vec![],
    };

    let user_prompt = generate_missing_fields_prompt(&result.missing_parameters);

    let usage_info = UsageInfo {
        input_tokens: 30,
        output_tokens: 15,
        total_tokens: 45,
        model: "progressive_matching".to_string(),
        estimated: true,
    };

    Ok(EnhancedAnalysisResult {
        endpoint_id: endpoint.id.clone(),
        endpoint_name: endpoint.name.clone(),
        endpoint_description: endpoint.description.clone(),
        verb: endpoint.verb.clone(),
        base: endpoint.base.clone(),
        path: endpoint.path.clone(),
        essential_path: endpoint.essential_path.clone(),
        api_group_id: endpoint.api_group_id.clone(),
        api_group_name: endpoint.api_group_name.clone(),
        parameters,
        raw_json: serde_json::json!({
            "type": "progressive_partial",
            "endpoint_id": endpoint.id,
            "status": "incomplete",
            "completion_percentage": result.completion_percentage,
            "missing_parameters": result.missing_parameters
        }),
        conversation_id: conversation_id.clone(),
        matching_info,
        user_prompt: Some(user_prompt),
        total_input_tokens: usage_info.input_tokens,
        total_output_tokens: usage_info.output_tokens,
        usage: usage_info,
        intent: IntentType::ActionableRequest,
    })
}

pub fn generate_missing_fields_prompt(missing_params: &[String]) -> String {
    match missing_params.len() {
        0 => "All required information has been provided.".to_string(),
        1 => format!(
            "I need one more piece of information: {}. Could you please provide it?",
            missing_params[0].replace('_', " ")
        ),
        2 => format!(
            "I need two more pieces of information: {} and {}. Could you provide them?",
            missing_params[0].replace('_', " "),
            missing_params[1].replace('_', " ")
        ),
        _ => {
            let (initial, last) = missing_params.split_at(missing_params.len() - 1);
            format!(
                "I need a few more details: {}, and {}. Could you provide this information?",
                initial
                    .iter()
                    .map(|p| p.replace('_', " "))
                    .collect::<Vec<_>>()
                    .join(", "),
                last[0].replace('_', " ")
            )
        }
    }
}

pub async fn create_fallback_response(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    model: String,
    conversation_id: Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let conversational_result = handle_general_question(sentence, provider).await?;

    let matching_info = MatchingInfo {
        status: MatchingStatus::Complete,
        total_required_fields: 0,
        mapped_required_fields: 0,
        total_optional_fields: 0,
        mapped_optional_fields: 0,
        completion_percentage: 100.0,
        missing_required_fields: vec![],
        missing_optional_fields: vec![],
    };

    let usage_info = UsageInfo {
        input_tokens: conversational_result.usage.input_tokens,
        output_tokens: conversational_result.usage.output_tokens,
        total_tokens: conversational_result.usage.total_tokens,
        model,
        estimated: conversational_result.usage.estimated,
    };

    Ok(EnhancedAnalysisResult {
        endpoint_id: "general_conversation_fallback".to_string(),
        endpoint_name: "General Conversation (Fallback)".to_string(),
        endpoint_description: "Fallback conversational response after endpoint matching failed"
            .to_string(),
        verb: "GET".to_string(),
        base: "conversation".to_string(),
        path: "/general".to_string(),
        essential_path: "/general".to_string(),
        api_group_id: "conversation".to_string(),
        api_group_name: "Conversation API".to_string(),
        parameters: vec![],
        raw_json: serde_json::json!({
            "type": "general_conversation_fallback",
            "response": conversational_result.content,
            "intent": "actionable_request_failed",
            "fallback_reason": "endpoint_matching_failed_after_retries"
        }),
        conversation_id,
        matching_info,
        user_prompt: None,
        total_input_tokens: conversational_result.usage.input_tokens,
        total_output_tokens: conversational_result.usage.output_tokens,
        usage: usage_info,
        intent: IntentType::GeneralQuestion,
    })
}

pub async fn create_help_response(
    sentence: &str,
    enhanced_endpoints: &[EnhancedEndpoint],
    provider: Arc<dyn ModelProvider>,
    conversation_id: Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let help_result = handle_help_request(sentence, enhanced_endpoints, provider.clone()).await?;

    let matching_info = MatchingInfo {
        status: MatchingStatus::Complete,
        total_required_fields: 0,
        mapped_required_fields: 0,
        total_optional_fields: 0,
        mapped_optional_fields: 0,
        completion_percentage: 100.0,
        missing_required_fields: vec![],
        missing_optional_fields: vec![],
    };

    let usage_info = UsageInfo {
        input_tokens: help_result.usage.input_tokens,
        output_tokens: help_result.usage.output_tokens,
        total_tokens: help_result.usage.total_tokens,
        model: provider.get_model_name().to_string(),
        estimated: help_result.usage.estimated,
    };

    Ok(EnhancedAnalysisResult {
        endpoint_id: "help_capabilities".to_string(),
        endpoint_name: "Help - Available Capabilities".to_string(),
        endpoint_description: "List of available system capabilities and how to use them"
            .to_string(),
        verb: "GET".to_string(),
        base: "help".to_string(),
        path: "/capabilities".to_string(),
        essential_path: "/capabilities".to_string(),
        api_group_id: "help".to_string(),
        api_group_name: "Help System".to_string(),
        parameters: vec![],
        raw_json: serde_json::json!({
            "type": "help_request",
            "response": help_result.content,
            "intent": "help_request",
            "capabilities_count": enhanced_endpoints.len()
        }),
        conversation_id,
        matching_info,
        user_prompt: None,
        total_input_tokens: usage_info.input_tokens,
        total_output_tokens: usage_info.output_tokens,
        usage: usage_info,
        intent: IntentType::HelpRequest,
    })
}

pub async fn create_general_response(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    model: String,
    conversation_id: Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let conversational_result = handle_general_question(sentence, provider.clone()).await?;

    let matching_info = MatchingInfo {
        status: MatchingStatus::Complete,
        total_required_fields: 0,
        mapped_required_fields: 0,
        total_optional_fields: 0,
        mapped_optional_fields: 0,
        completion_percentage: 100.0,
        missing_required_fields: vec![],
        missing_optional_fields: vec![],
    };

    let usage_info = UsageInfo {
        input_tokens: conversational_result.usage.input_tokens,
        output_tokens: conversational_result.usage.output_tokens,
        total_tokens: conversational_result.usage.total_tokens,
        model,
        estimated: conversational_result.usage.estimated,
    };

    Ok(EnhancedAnalysisResult {
        endpoint_id: "general_conversation".to_string(),
        endpoint_name: "General Conversation".to_string(),
        endpoint_description: "Conversational response to general question".to_string(),
        verb: "GET".to_string(),
        base: "conversation".to_string(),
        path: "/general".to_string(),
        essential_path: "/general".to_string(),
        api_group_id: "conversation".to_string(),
        api_group_name: "Conversation API".to_string(),
        parameters: vec![],
        raw_json: serde_json::json!({
            "type": "general_conversation",
            "response": conversational_result.content,
            "intent": "general_question"
        }),
        conversation_id,
        matching_info,
        user_prompt: None,
        total_input_tokens: usage_info.input_tokens,
        total_output_tokens: usage_info.output_tokens,
        usage: usage_info,
        intent: IntentType::GeneralQuestion,
    })
}
