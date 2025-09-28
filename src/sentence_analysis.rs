// src/sentence_analysis.rs
use crate::analyze_sentence::analyze_sentence_enhanced;
use crate::conversation::ConversationManager;
use crate::models::providers::ModelProvider;
use crate::progressive_matching::{ParameterValue, ProgressiveMatchingManager};
use crate::workflow::classify_intent::IntentType;
use std::sync::Arc;
use tonic::Status;
use tracing::Instrument;

use crate::sentence_service::sentence::{
    IntentType as ProtoIntentType, MatchingInfo, MatchingStatus, MissingField, Parameter,
    SentenceResponse, Usage,
};

#[derive(Clone)]
pub struct SentenceAnalyzer {
    pub provider: Arc<dyn ModelProvider>,
    pub api_url: Option<String>,
    pub conversation_manager: Arc<ConversationManager>,
    progressive_manager: Option<Arc<ProgressiveMatchingManager>>,
}

impl SentenceAnalyzer {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        api_url: Option<String>,
        conversation_manager: Arc<ConversationManager>,
        progressive_manager: Option<Arc<ProgressiveMatchingManager>>,
    ) -> Self {
        Self {
            provider,
            api_url,
            conversation_manager,
            progressive_manager,
        }
    }

    pub async fn analyze_sentence_stream(
        &self,
        input_sentence: String,
        conversation_id: String,
        email: String,
        client_id: String,
        tx: tokio::sync::mpsc::Sender<Result<SentenceResponse, Status>>,
    ) {
        let analyze_span = tracing::info_span!(
            "analyze_sentence",
            client_id = %client_id,
            email = %email,
            conversation_id = %conversation_id
        );

        let model = self.provider.get_model_name().to_string();
        let provider_clone = self.provider.clone();
        let api_url_clone = self.api_url.clone();
        let conversation_manager_clone = self.conversation_manager.clone();
        let progressive_manager_clone = self.progressive_manager.clone();

        // Check for progressive matching FIRST
        if let Some(ref manager) = progressive_manager_clone {
            if let Some(response) = self
                .handle_progressive_followup(
                    &input_sentence,
                    &conversation_id,
                    provider_clone.clone(),
                    manager,
                    api_url_clone.as_ref().unwrap_or(&"".to_string()),
                    &email,
                )
                .await
                .unwrap_or(None)
            {
                if tx.send(Ok(response)).await.is_err() {
                    tracing::error!("Failed to send progressive response");
                }
                return; // Early return for progressive matching
            }
        }

        // Only if no progressive match found, do normal analysis
        let result = analyze_sentence_enhanced(
            &input_sentence,
            provider_clone,
            api_url_clone,
            &email,
            Some(conversation_id.clone()),
        )
        .instrument(analyze_span)
        .await;

        match result {
            Ok(enhanced_result) => {
                self.handle_successful_analysis(
                    enhanced_result,
                    input_sentence,
                    conversation_id,
                    email,
                    client_id,
                    model,
                    tx,
                    conversation_manager_clone,
                    progressive_manager_clone,
                )
                .await;
            }
            Err(e) => {
                self.handle_analysis_error(
                    e,
                    input_sentence,
                    conversation_id,
                    email,
                    client_id,
                    tx,
                )
                .await;
            }
        }
    }

    async fn handle_successful_analysis(
        &self,
        enhanced_result: crate::models::EnhancedAnalysisResult,
        input_sentence: String,
        conversation_id: String,
        email: String,
        client_id: String,
        model: String,
        tx: tokio::sync::mpsc::Sender<Result<SentenceResponse, Status>>,
        conversation_manager: Arc<ConversationManager>,
        progressive_manager: Option<Arc<ProgressiveMatchingManager>>,
    ) {
        tracing::info!(
            client_id = %client_id,
            email = %email,
            conversation_id = %conversation_id,
            total_input_tokens = enhanced_result.total_input_tokens,
            total_output_tokens = enhanced_result.total_output_tokens,
            "Analysis completed"
        );

        // Progressive matching integration for NEW requests
        if let Some(ref manager) = progressive_manager {
            self.save_incomplete_request_if_needed(
                &enhanced_result,
                &conversation_id,
                &email,
                manager,
            )
            .await;
        }

        // Add message to conversation history
        self.save_to_conversation_history(
            &enhanced_result,
            &input_sentence,
            &conversation_id,
            conversation_manager,
        )
        .await;

        // Build and send response
        let response =
            self.build_sentence_response(enhanced_result, conversation_id.clone(), model);

        if tx.send(Ok(response)).await.is_err() {
            tracing::error!(
                client_id = %client_id,
                email = %email,
                conversation_id = %conversation_id,
                "Failed to send response - stream closed"
            );
        }
    }

    async fn handle_analysis_error(
        &self,
        error: Box<dyn std::error::Error + Send + Sync>,
        input_sentence: String,
        conversation_id: String,
        email: String,
        client_id: String,
        tx: tokio::sync::mpsc::Sender<Result<SentenceResponse, Status>>,
    ) {
        tracing::error!(
            input_sentence = %input_sentence,
            error = %error,
            client_id = %client_id,
            email = %email,
            conversation_id = %conversation_id,
            "Analysis failed"
        );

        let status = if error.to_string().contains("No endpoints found for user") {
            Status::not_found(format!(
                "No endpoints configured for your account ({email}). Please contact your administrator."
            ))
        } else if error
            .to_string()
            .contains("No endpoint configuration available")
        {
            Status::failed_precondition("Endpoint configuration is not available.")
        } else {
            Status::internal(format!("Analysis failed: {error}"))
        };

        if tx.send(Err(status)).await.is_err() {
            tracing::error!("Failed to send error response - stream closed");
        }
    }

    async fn save_incomplete_request_if_needed(
        &self,
        enhanced_result: &crate::models::EnhancedAnalysisResult,
        conversation_id: &str,
        email: &str, // Add email parameter
        manager: &Arc<ProgressiveMatchingManager>,
    ) {
        if enhanced_result.intent == IntentType::ActionableRequest
            && enhanced_result.matching_info.completion_percentage < 100.0
        {
            let temp = "".to_string();
            // Get the actual endpoint to access its parameter definitions
            let api_url = self.api_url.as_ref().unwrap_or(&temp);

            match crate::endpoint_client::get_enhanced_endpoints(api_url, email).await {
                Ok(enhanced_endpoints) => {
                    if let Some(endpoint) = enhanced_endpoints
                        .iter()
                        .find(|e| e.id == enhanced_result.endpoint_id)
                    {
                        // Get required parameter names
                        let required_param_names: Vec<String> = endpoint
                            .parameters
                            .iter()
                            .filter(|p| p.required.unwrap_or(false))
                            .map(|p| p.name.clone())
                            .collect();

                        // Convert matched parameters to ParameterValue format
                        let new_parameters: Vec<ParameterValue> = enhanced_result
                            .parameters
                            .iter()
                            .filter_map(|p| {
                                p.value.as_ref().map(|val| ParameterValue {
                                    name: p.name.clone(),
                                    value: val.clone(),
                                    description: p.description.clone(),
                                })
                            })
                            .collect();

                        match crate::progressive_matching::integrate_progressive_matching(
                            conversation_id,
                            &enhanced_result.endpoint_id,
                            new_parameters,
                            required_param_names,
                            manager,
                            &endpoint.parameters, // Now endpoint is in scope
                        )
                        .await
                        {
                            Ok(progressive_result) => {
                                tracing::info!(
                                "Saved incomplete request to progressive matching: {}% complete",
                                progressive_result.completion_percentage
                            );
                            }
                            Err(e) => {
                                tracing::warn!("Progressive matching failed: {}", e);
                            }
                        }
                    } else {
                        tracing::error!(
                            "Endpoint {} not found for progressive matching",
                            enhanced_result.endpoint_id
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to get enhanced endpoints for progressive matching: {}",
                        e
                    );
                }
            }
        }
    }

    async fn save_to_conversation_history(
        &self,
        enhanced_result: &crate::models::EnhancedAnalysisResult,
        input_sentence: &str,
        conversation_id: &str,
        conversation_manager: Arc<ConversationManager>,
    ) {
        let parameters_json =
            serde_json::to_value(&enhanced_result.parameters).unwrap_or(serde_json::Value::Null);

        if let Err(e) = conversation_manager
            .add_message(
                conversation_id,
                input_sentence.to_string(),
                Some(enhanced_result.endpoint_id.clone()),
                Some(parameters_json),
            )
            .await
        {
            tracing::warn!("Failed to save message to conversation history: {}", e);
        }
    }

    fn build_sentence_response(
        &self,
        enhanced_result: crate::models::EnhancedAnalysisResult,
        conversation_id: String,
        model: String,
    ) -> SentenceResponse {
        let usage_info = crate::models::UsageInfo {
            input_tokens: enhanced_result.usage.input_tokens,
            output_tokens: enhanced_result.usage.output_tokens,
            total_tokens: enhanced_result.usage.total_tokens,
            model,
            estimated: enhanced_result.usage.estimated,
        };

        SentenceResponse {
            conversation_id: Some(conversation_id),
            endpoint_id: enhanced_result.endpoint_id,
            endpoint_name: Some(enhanced_result.endpoint_name),
            endpoint_description: enhanced_result.endpoint_description,
            verb: Some(enhanced_result.verb),
            base: Some(enhanced_result.base),
            path: Some(enhanced_result.path),
            essential_path: Some(enhanced_result.essential_path),
            api_group_id: Some(enhanced_result.api_group_id),
            api_group_name: Some(enhanced_result.api_group_name),
            user_prompt: enhanced_result.user_prompt,
            usage: Some(Usage {
                input_tokens: usage_info.input_tokens,
                output_tokens: usage_info.output_tokens,
                total_tokens: usage_info.total_tokens,
                model: usage_info.model,
                estimated: usage_info.estimated,
            }),
            intent: match enhanced_result.intent {
                IntentType::ActionableRequest => ProtoIntentType::ActionableRequest as i32,
                IntentType::GeneralQuestion => ProtoIntentType::GeneralQuestion as i32,
                IntentType::HelpRequest => ProtoIntentType::HelpRequest as i32,
            },
            parameters: enhanced_result
                .parameters
                .into_iter()
                .map(|param| Parameter {
                    name: param.name,
                    description: param.description,
                    semantic_value: param.value,
                })
                .collect(),
            json_output: match serde_json::to_string(&enhanced_result.raw_json) {
                Ok(json) => json,
                Err(e) => {
                    tracing::error!(error = %e, "JSON serialization failed");
                    format!("{{\"error\": \"JSON serialization failed: {e}\"}}")
                }
            },
            matching_info: Some(MatchingInfo {
                status: match enhanced_result.matching_info.status {
                    crate::models::MatchingStatus::Complete => MatchingStatus::Complete as i32,
                    crate::models::MatchingStatus::Partial => MatchingStatus::Partial as i32,
                    crate::models::MatchingStatus::Incomplete => MatchingStatus::Incomplete as i32,
                },
                total_required_fields: enhanced_result.matching_info.total_required_fields as i32,
                mapped_required_fields: enhanced_result.matching_info.mapped_required_fields as i32,
                total_optional_fields: enhanced_result.matching_info.total_optional_fields as i32,
                mapped_optional_fields: enhanced_result.matching_info.mapped_optional_fields as i32,
                completion_percentage: enhanced_result.matching_info.completion_percentage,
                missing_required_fields: enhanced_result
                    .matching_info
                    .missing_required_fields
                    .into_iter()
                    .map(|field| MissingField {
                        name: field.name,
                        description: field.description,
                    })
                    .collect(),
                missing_optional_fields: enhanced_result
                    .matching_info
                    .missing_optional_fields
                    .into_iter()
                    .map(|field| MissingField {
                        name: field.name,
                        description: field.description,
                    })
                    .collect(),
            }),
        }
    }

    // Progressive matching helper functions
    pub async fn handle_progressive_followup(
        &self,
        sentence: &str,
        conversation_id: &str,
        provider: Arc<dyn ModelProvider>,
        progressive_manager: &ProgressiveMatchingManager,
        api_url: &str,
        email: &str,
    ) -> Result<Option<SentenceResponse>, Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(
            "Progressive check: conversation_id='{}', sentence='{}'",
            conversation_id,
            sentence
        );

        // Check if there's an ongoing incomplete request
        match progressive_manager
            .get_incomplete_match(conversation_id)
            .await
        {
            Ok(Some(ongoing_match)) => {
                tracing::info!(
                    "Found ongoing match: endpoint='{}', stored_params='{}'",
                    ongoing_match.endpoint_id,
                    ongoing_match.parameters
                );

                // Get endpoint information FIRST - move this up
                let enhanced_endpoints =
                    match crate::endpoint_client::get_enhanced_endpoints(api_url, email).await {
                        Ok(endpoints) => endpoints,
                        Err(e) => {
                            tracing::error!("Failed to get enhanced endpoints: {}", e);
                            return Err(e);
                        }
                    };

                let endpoint = enhanced_endpoints
                    .iter()
                    .find(|e| e.id == ongoing_match.endpoint_id)
                    .ok_or_else(|| {
                        tracing::error!(
                            "Endpoint '{}' not found in available endpoints",
                            ongoing_match.endpoint_id
                        );
                        "Endpoint not found"
                    })?;

                tracing::info!(
                    "Found endpoint: {} with {} parameters",
                    endpoint.name,
                    endpoint.parameters.len()
                );

                // Parse existing parameters to see what we already have
                let existing_params: Vec<ParameterValue> =
                    match serde_json::from_str(&ongoing_match.parameters) {
                        Ok(params) => params,
                        Err(e) => {
                            tracing::error!("Failed to parse existing parameters: {}", e);
                            Vec::new()
                        }
                    };

                tracing::info!("Existing parameters count: {}", existing_params.len());
                for param in &existing_params {
                    tracing::info!("  - {}: {}", param.name, param.value);
                }

                // NOW extract new parameters - endpoint is in scope
                let new_params = extract_parameters_from_followup(
                    sentence,
                    provider.clone(),
                    &endpoint.parameters, // Now endpoint is available
                )
                .await?;

                tracing::info!("Extracted {} new parameters", new_params.len());
                for param in &new_params {
                    tracing::info!("  + {}: {}", param.name, param.value);
                }

                if new_params.is_empty() {
                    tracing::debug!(
                        "No parameters extracted from follow-up, treating as new request"
                    );
                    return Ok(None);
                }

                // Update the progressive match with new parameters
                progressive_manager
                    .update_match(
                        conversation_id,
                        &ongoing_match.endpoint_id,
                        new_params.clone(),
                    )
                    .await?;

                tracing::info!(
                    "Updated progressive match with {} new parameters",
                    new_params.len()
                );

                // Check completion status
                let required_params: Vec<String> = endpoint
                    .parameters
                    .iter()
                    .filter(|p| p.required.unwrap_or(false))
                    .map(|p| p.name.clone())
                    .collect();

                tracing::info!("Required parameters for completion: {:?}", required_params);

                let completion_result = progressive_manager
                    .check_completion(
                        conversation_id,
                        &ongoing_match.endpoint_id,
                        required_params,
                        &endpoint.parameters,
                    )
                    .await?;

                tracing::info!(
                    "Completion check: {}% complete, is_complete: {}, missing: {:?}",
                    completion_result.completion_percentage,
                    completion_result.is_complete,
                    completion_result.missing_parameters
                );

                if completion_result.is_complete {
                    tracing::info!("Progressive match is complete, cleaning up");

                    // Clean up and return complete result
                    progressive_manager
                        .complete_match(conversation_id, &ongoing_match.endpoint_id)
                        .await?;

                    return Ok(Some(build_complete_progressive_response(
                        endpoint,
                        completion_result,
                        conversation_id,
                    )));
                } else {
                    tracing::info!(
                        "Progressive match still incomplete, returning partial response"
                    );

                    // Return partial result with user prompt
                    return Ok(Some(build_partial_progressive_response(
                        endpoint,
                        completion_result,
                        conversation_id,
                    )));
                }
            }
            Ok(None) => {
                tracing::info!(
                    "No ongoing progressive match found for conversation: {}",
                    conversation_id
                );
            }
            Err(e) => {
                tracing::error!("Error checking for progressive match: {}", e);
                // Don't fail the entire request, just continue with normal processing
            }
        }

        Ok(None) // No ongoing match found or error occurred
    }
}

async fn extract_parameters_from_followup(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    endpoint_parameters: &[crate::models::EndpointParameter],
) -> Result<Vec<ParameterValue>, Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!("🔍 Extracting parameters from: '{}'", sentence);
    tracing::info!(
        "🔍 Available endpoint parameters: {:?}",
        endpoint_parameters
            .iter()
            .map(|p| &p.name)
            .collect::<Vec<_>>()
    );

    let prompt_manager = crate::prompts::PromptManager::new().await?;

    // Create the available parameters list for the prompt
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

    let models_config = crate::models::config::load_models_config().await?;
    let model_config = &models_config.default;

    let result = provider.generate(&prompt, model_config).await?;
    let json_result = crate::json_helper::sanitize_json(&result.content)?;

    let mut parameters = Vec::new();

    // Get valid parameter names from endpoint specification
    let valid_param_names: Vec<&str> = endpoint_parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();

    if let Some(obj) = json_result.as_object() {
        for (key, value) in obj {
            if let Some(str_value) = value.as_str() {
                if !str_value.trim().is_empty() {
                    // Only add if it's a valid parameter name from the spec
                    if valid_param_names.contains(&key.as_str()) {
                        parameters.push(ParameterValue {
                            name: key.clone(),
                            value: str_value.trim().to_string(),
                            description: format!("User provided value for {key}"),
                        });
                    } else {
                        tracing::warn!(
                            "LLM returned invalid parameter '{}' not in endpoint specification",
                            key
                        );
                    }
                }
            }
        }
    }

    // No fallback - if LLM can't map it to valid parameters, return empty
    Ok(parameters)
}

fn build_complete_progressive_response(
    endpoint: &crate::models::EnhancedEndpoint,
    result: crate::progressive_matching::ProgressiveMatchResult,
    conversation_id: &str,
) -> SentenceResponse {
    let matched_params_len = result.matched_parameters.len();
    SentenceResponse {
        conversation_id: Some(conversation_id.to_string()),
        endpoint_id: endpoint.id.clone(),
        endpoint_name: Some(endpoint.name.clone()),
        endpoint_description: endpoint.description.clone(),
        verb: Some(endpoint.verb.clone()),
        base: Some(endpoint.base.clone()),
        path: Some(endpoint.path.clone()),
        essential_path: Some(endpoint.essential_path.clone()),
        api_group_id: Some(endpoint.api_group_id.clone()),
        api_group_name: Some(endpoint.api_group_name.clone()),
        user_prompt: None, // Complete, so no more prompting needed
        usage: Some(Usage {
            input_tokens: 50, // Estimated for progressive matching
            output_tokens: 20,
            total_tokens: 70,
            model: "progressive_matching".to_string(),
            estimated: true,
        }),
        intent: ProtoIntentType::ActionableRequest as i32,
        parameters: result
            .matched_parameters
            .into_iter()
            .map(|param| Parameter {
                name: param.name,
                description: param.description,
                semantic_value: Some(param.value),
            })
            .collect(),
        json_output: serde_json::json!({
            "type": "progressive_complete",
            "endpoint_id": endpoint.id,
            "status": "complete",
            "completion_percentage": 100.0
        })
        .to_string(),
        matching_info: Some(MatchingInfo {
            status: MatchingStatus::Complete as i32,
            total_required_fields: matched_params_len as i32,
            mapped_required_fields: matched_params_len as i32,
            total_optional_fields: 0,
            mapped_optional_fields: 0,
            completion_percentage: 100.0,
            missing_required_fields: vec![],
            missing_optional_fields: vec![],
        }),
    }
}

fn build_partial_progressive_response(
    endpoint: &crate::models::EnhancedEndpoint,
    result: crate::progressive_matching::ProgressiveMatchResult,
    conversation_id: &str,
) -> SentenceResponse {
    let user_prompt = generate_missing_fields_prompt(&result.missing_parameters);

    let matched_params_len = result.matched_parameters.len();
    let missing_params_len = result.missing_parameters.len();
    let completion_percentage = if (matched_params_len + missing_params_len) > 0 {
        (matched_params_len as f32 / (matched_params_len + missing_params_len) as f32) * 100.0
    } else {
        0.0
    };
    let missing_parameters = result.missing_parameters.clone();

    SentenceResponse {
        conversation_id: Some(conversation_id.to_string()),
        endpoint_id: endpoint.id.clone(),
        endpoint_name: Some(endpoint.name.clone()),
        endpoint_description: endpoint.description.clone(),
        verb: Some(endpoint.verb.clone()),
        base: Some(endpoint.base.clone()),
        path: Some(endpoint.path.clone()),
        essential_path: Some(endpoint.essential_path.clone()),
        api_group_id: Some(endpoint.api_group_id.clone()),
        api_group_name: Some(endpoint.api_group_name.clone()),
        user_prompt: Some(user_prompt),
        usage: Some(Usage {
            input_tokens: 30,
            output_tokens: 15,
            total_tokens: 45,
            model: "progressive_matching".to_string(),
            estimated: true,
        }),
        intent: ProtoIntentType::ActionableRequest as i32,
        parameters: result
            .matched_parameters
            .into_iter()
            .map(|param| Parameter {
                name: param.name,
                description: param.description,
                semantic_value: Some(param.value),
            })
            .collect(),
        json_output: serde_json::json!({
            "type": "progressive_partial",
            "endpoint_id": endpoint.id,
            "status": "incomplete",
            "completion_percentage": completion_percentage,
            "missing_parameters": missing_parameters
        })
        .to_string(),
        matching_info: Some(MatchingInfo {
            status: MatchingStatus::Partial as i32,
            total_required_fields: (matched_params_len + missing_params_len) as i32,
            mapped_required_fields: matched_params_len as i32,
            total_optional_fields: 0,
            mapped_optional_fields: 0,
            completion_percentage,
            missing_required_fields: missing_parameters
                .into_iter()
                .map(|param| MissingField {
                    name: param.clone(),
                    description: format!("Missing required parameter: {param}"),
                })
                .collect(),
            missing_optional_fields: vec![],
        }),
    }
}

fn generate_missing_fields_prompt(missing_params: &[String]) -> String {
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
