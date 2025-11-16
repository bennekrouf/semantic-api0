// src/sentence_analysis.rs
use crate::conversation::ConversationManager;
use crate::models::providers::ModelProvider;
use crate::models::EnhancedAnalysisResult;
use crate::progressive_matching::{integrate_progressive_matching, ParameterValue, ProgressiveMatchingManager};
use crate::workflow::classify_intent::IntentType;
use crate::analysis::analyze_sentence_enhanced::analyze_sentence_enhanced;

use std::sync::Arc;
use graflog::app_span;
use tonic::Status;
use crate::app_log;
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
        let analyze_span = app_span!(
            "analyze_sentence",
            client_id = %client_id,
            email = %email,
            conversation_id = %conversation_id
        );

        let _enter = analyze_span.enter();

        let model = self.provider.get_model_name().to_string();
        let provider_clone = self.provider.clone();
        let api_url_clone = self.api_url.clone();
        let conversation_manager_clone = self.conversation_manager.clone();
        let progressive_manager_clone = self.progressive_manager.clone();

        // Only if no progressive match found, do normal analysis
        let result = analyze_sentence_enhanced(
                &input_sentence,
                provider_clone,
                api_url_clone,
                &email,
                Some(conversation_id.clone()),
            )
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
        app_log!(info, 
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
            app_log!(error, 
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
        app_log!(error, 
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
            app_log!(error, "Failed to send error response - stream closed");
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

                        match integrate_progressive_matching(
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
                                app_log!(info, 
                                "Saved incomplete request to progressive matching: {}% complete",
                                progressive_result.completion_percentage
                            );
                            }
                            Err(e) => {
                                app_log!(warn, "Progressive matching failed: {}", e);
                            }
                        }
                    } else {
                        app_log!(error, 
                            "Endpoint {} not found for progressive matching",
                            enhanced_result.endpoint_id
                        );
                    }
                }
                Err(e) => {
                    app_log!(error, 
                        "Failed to get enhanced endpoints for progressive matching: {}",
                        e
                    );
                }
            }
        }
    }

    async fn save_to_conversation_history(
        &self,
        enhanced_result: &EnhancedAnalysisResult,
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
            app_log!(warn, "Failed to save message to conversation history: {}", e);
        }
    }

    fn build_sentence_response(
        &self,
        enhanced_result: EnhancedAnalysisResult,
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

        // Clone endpoint_id once for reuse
        let endpoint_id = enhanced_result.endpoint_id.clone();

        SentenceResponse {
            conversation_id: Some(conversation_id),
            endpoint_id: endpoint_id.clone(), // Use the clone
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
                endpoint_path: "/api/analyze".to_string(),
                method: "POST".to_string(),
                matched_endpoint_id: Some(endpoint_id), // Use the clone
                user_sentence: None,
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
                    app_log!(error, error = %e, "JSON serialization failed");
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
}
