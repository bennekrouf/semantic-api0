// src/sentence_service.rs
use crate::analyze_sentence::analyze_sentence_enhanced;
use crate::conversation::ConversationManager;
use crate::models::providers::ModelProvider;
use crate::progressive_matching::{
    integrate_progressive_matching, ParameterValue, ProgressiveMatchingManager,
};
use crate::workflow::classify_intent::IntentType;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};

pub mod sentence {
    tonic::include_proto!("sentence");
}

use sentence::sentence_service_server::SentenceService;
use sentence::{MessageRequest, MessageResponse, Parameter, SentenceRequest, SentenceResponse};
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use tracing::Instrument;

pub struct SentenceAnalyzeService {
    provider: Arc<dyn ModelProvider>,
    api_url: Option<String>,
    conversation_manager: Arc<ConversationManager>,
    progressive_manager: Option<Arc<ProgressiveMatchingManager>>,
}

impl SentenceAnalyzeService {
    pub fn new(provider: Arc<dyn ModelProvider>, api_url: Option<String>) -> Self {
        Self {
            provider,
            api_url,
            conversation_manager: Arc::new(ConversationManager::new()),
            progressive_manager: None,
        }
    }

    pub async fn with_progressive_matching(
        provider: Arc<dyn ModelProvider>,
        api_url: Option<String>,
        database_url: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let progressive_manager = Arc::new(ProgressiveMatchingManager::new(database_url).await?);

        Ok(Self {
            provider,
            api_url,
            conversation_manager: Arc::new(ConversationManager::new()),
            progressive_manager: Some(progressive_manager),
        })
    }

    fn get_email_validated(&self, metadata: &MetadataMap) -> Result<String, tonic::Status> {
        let email = metadata
            .get("email")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                tonic::Status::invalid_argument(
                    "Email is required in request metadata. Add 'email' header to your request.",
                )
            })?
            .to_string();

        match crate::utils::email::validate_email(&email) {
            Ok(_) => Ok(email),
            Err(e) => Err(tonic::Status::invalid_argument(format!(
                "Email validation failed: {}",
                e
            ))),
        }
    }

    fn get_client_id(metadata: &MetadataMap) -> String {
        metadata
            .get("client-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown-client")
            .to_string()
    }

    async fn ensure_conversation_id(
        &self,
        conversation_id: Option<String>,
        email: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        match conversation_id {
            Some(id) if !id.is_empty() => {
                if self
                    .conversation_manager
                    .get_conversation(&id)
                    .await
                    .is_some()
                {
                    Ok(id)
                } else {
                    tracing::warn!("Conversation {} not found, creating new one", id);
                    self.conversation_manager
                        .start_conversation(email.to_string(), self.api_url.clone())
                        .await
                }
            }
            _ => {
                self.conversation_manager
                    .start_conversation(email.to_string(), self.api_url.clone())
                    .await
            }
        }
    }
}

impl std::fmt::Debug for SentenceAnalyzeService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SentenceAnalyzeService")
            .field("provider", &"<dyn ModelProvider>")
            .field("api_url", &self.api_url)
            .finish()
    }
}

#[tonic::async_trait]
impl SentenceService for SentenceAnalyzeService {
    type AnalyzeSentenceStream =
        Pin<Box<dyn Stream<Item = Result<SentenceResponse, Status>> + Send>>;

    #[tracing::instrument(skip(self, request), fields(client_id, email, conversation_id))]
    async fn analyze_sentence(
        &self,
        request: Request<SentenceRequest>,
    ) -> Result<Response<Self::AnalyzeSentenceStream>, Status> {
        let metadata = request.metadata().clone();
        let sentence_request = request.into_inner();

        tracing::info!("Request metadata: {:?}", metadata);

        let client_id = Self::get_client_id(&metadata);
        let email = match self.get_email_validated(&metadata) {
            Ok(email) => email,
            Err(status) => {
                tracing::error!("Email validation failed: {}", status);
                return Err(status);
            }
        };

        let input_sentence = sentence_request.sentence;

        let conversation_id = match self
            .ensure_conversation_id(sentence_request.conversation_id.clone(), &email)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("Failed to ensure conversation_id: {}", e);
                return Err(Status::internal("Failed to manage conversation"));
            }
        };

        tracing::info!(
            input_sentence = %input_sentence,
            email = %email,
            conversation_id = %conversation_id,
            "Processing sentence request"
        );

        let (tx, rx) = mpsc::channel(10);
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
        let email_clone = email.clone();
        let conversation_id_clone = conversation_id.clone();

        tokio::spawn(async move {
            let result = analyze_sentence_enhanced(
                &input_sentence,
                provider_clone,
                api_url_clone,
                &email_clone,
                Some(conversation_id_clone.clone()),
            )
            .instrument(analyze_span)
            .await;

            match result {
                Ok(enhanced_result) => {
                    tracing::info!(
                        client_id = %client_id,
                        email = %email_clone,
                        conversation_id = %conversation_id_clone,
                        total_input_tokens = enhanced_result.total_input_tokens,
                        total_output_tokens = enhanced_result.total_output_tokens,
                        "Analysis completed"
                    );

                    // Progressive matching integration
                    let progressive_result = if let Some(ref manager) = progressive_manager_clone {
                        // Get required parameter names from endpoint
                        let all_endpoint_params: Vec<String> = enhanced_result
                            .matching_info
                            .missing_required_fields
                            .iter()
                            .map(|f| f.name.clone())
                            .chain(enhanced_result.parameters.iter().map(|p| p.name.clone()))
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
                            &conversation_id_clone,
                            &enhanced_result.endpoint_id,
                            new_parameters,
                            all_endpoint_params,
                            manager,
                        )
                        .await
                        {
                            Ok(progressive_result) => Some(progressive_result),
                            Err(e) => {
                                tracing::warn!("Progressive matching failed: {}", e);
                                None
                            }
                        }
                    } else {
                        None
                    };

                    // Add message to conversation history
                    let parameters_json = serde_json::to_value(&enhanced_result.parameters)
                        .unwrap_or(serde_json::Value::Null);

                    if let Err(e) = conversation_manager_clone
                        .add_message(
                            &conversation_id_clone,
                            input_sentence.clone(),
                            Some(enhanced_result.endpoint_id.clone()),
                            Some(parameters_json),
                        )
                        .await
                    {
                        tracing::warn!("Failed to save message to conversation history: {}", e);
                    }

                    let usage_info = crate::models::UsageInfo {
                        input_tokens: enhanced_result.usage.input_tokens,
                        output_tokens: enhanced_result.usage.output_tokens,
                        total_tokens: enhanced_result.usage.total_tokens,
                        model,
                        estimated: enhanced_result.usage.estimated,
                    };

                    // In the analyze_sentence method, update the response construction:
                    let response = SentenceResponse {
                        conversation_id: Some(conversation_id_clone.clone()),
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
                        usage: Some(sentence::Usage {
                            input_tokens: usage_info.input_tokens,
                            output_tokens: usage_info.output_tokens,
                            total_tokens: usage_info.total_tokens,
                            model: usage_info.model,
                            estimated: usage_info.estimated,
                        }),
                        intent: match enhanced_result.intent {
                            // ADD THIS
                            IntentType::ActionableRequest => {
                                sentence::IntentType::ActionableRequest as i32
                            }
                            IntentType::GeneralQuestion => {
                                sentence::IntentType::GeneralQuestion as i32
                            }
                            IntentType::HelpRequest => sentence::IntentType::HelpRequest as i32,
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
                                format!("{{\"error\": \"JSON serialization failed: {}\"}}", e)
                            }
                        },
                        matching_info: Some(sentence::MatchingInfo {
                            status: match enhanced_result.matching_info.status {
                                crate::models::MatchingStatus::Complete => {
                                    sentence::MatchingStatus::Complete as i32
                                }
                                crate::models::MatchingStatus::Partial => {
                                    sentence::MatchingStatus::Partial as i32
                                }
                                crate::models::MatchingStatus::Incomplete => {
                                    sentence::MatchingStatus::Incomplete as i32
                                }
                            },
                            total_required_fields: enhanced_result
                                .matching_info
                                .total_required_fields
                                as i32,
                            mapped_required_fields: enhanced_result
                                .matching_info
                                .mapped_required_fields
                                as i32,
                            total_optional_fields: enhanced_result
                                .matching_info
                                .total_optional_fields
                                as i32,
                            mapped_optional_fields: enhanced_result
                                .matching_info
                                .mapped_optional_fields
                                as i32,
                            completion_percentage: enhanced_result
                                .matching_info
                                .completion_percentage,

                            // Clone and deduplicate missing required fields
                            missing_required_fields: {
                                let mut unique_missing: Vec<sentence::MissingField> = Vec::new();
                                let mut seen_names = std::collections::HashSet::new();

                                // Clone the missing required fields to avoid borrowing issues
                                for field in enhanced_result
                                    .matching_info
                                    .missing_required_fields
                                    .clone()
                                {
                                    if seen_names.insert(field.name.clone()) {
                                        unique_missing.push(sentence::MissingField {
                                            name: field.name,
                                            description: field.description,
                                        });
                                    } else {
                                        tracing::warn!(
                                            "Duplicate missing required field filtered: {}",
                                            field.name
                                        );
                                    }
                                }
                                unique_missing
                            },

                            // Clone and deduplicate missing optional fields
                            missing_optional_fields: {
                                let mut unique_missing: Vec<sentence::MissingField> = Vec::new();
                                let mut seen_names = std::collections::HashSet::new();

                                // Clone the missing optional fields to avoid borrowing issues
                                for field in enhanced_result
                                    .matching_info
                                    .missing_optional_fields
                                    .clone()
                                {
                                    if seen_names.insert(field.name.clone()) {
                                        unique_missing.push(sentence::MissingField {
                                            name: field.name,
                                            description: field.description,
                                        });
                                    } else {
                                        tracing::warn!(
                                            "Duplicate missing optional field filtered: {}",
                                            field.name
                                        );
                                    }
                                }
                                unique_missing
                            },
                        }),
                    };

                    tracing::debug!(
                        "Final response missing_required: {:?}",
                        enhanced_result
                            .matching_info
                            .missing_required_fields
                            .iter()
                            .map(|f| &f.name)
                            .collect::<Vec<_>>()
                    );
                    tracing::debug!(
                        "Final response missing_optional: {:?}",
                        enhanced_result
                            .matching_info
                            .missing_optional_fields
                            .iter()
                            .map(|f| &f.name)
                            .collect::<Vec<_>>()
                    );

                    if tx.send(Ok(response)).await.is_err() {
                        tracing::error!(
                            client_id = %client_id,
                            email = %email_clone,
                            conversation_id = %conversation_id_clone,
                            "Failed to send response - stream closed"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        input_sentence = %input_sentence,
                        error = %e,
                        client_id = %client_id,
                        email = %email_clone,
                        conversation_id = %conversation_id_clone,
                        "Analysis failed"
                    );

                    let status = if e.to_string().contains("No endpoints found for user") {
                        Status::not_found(format!(
                            "No endpoints configured for your account ({}). Please contact your administrator.",
                            email_clone
                        ))
                    } else if e
                        .to_string()
                        .contains("No endpoint configuration available")
                    {
                        Status::failed_precondition("Endpoint configuration is not available.")
                    } else {
                        Status::internal(format!("Analysis failed: {}", e))
                    };

                    if tx.send(Err(status)).await.is_err() {
                        tracing::error!("Failed to send error response - stream closed");
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    async fn send_message(
        &self,
        request: Request<MessageRequest>,
    ) -> Result<Response<MessageResponse>, Status> {
        let message_request = request.into_inner();
        let message = message_request.message;

        if message.trim().is_empty() {
            return Err(Status::invalid_argument("Message cannot be empty"));
        }

        let conversation_id = message_request
            .conversation_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        tracing::info!(
            message = %message,
            conversation_id = %conversation_id,
            "Processing message"
        );

        let models_config = match crate::models::config::load_models_config().await {
            Ok(config) => config,
            Err(e) => {
                tracing::error!("Failed to load models config: {}", e);
                return Err(Status::internal("Configuration error"));
            }
        };

        let model_config = &models_config.sentence_to_json;

        match self.provider.generate(&message, model_config).await {
            Ok(result) => {
                tracing::info!("Successfully generated response");
                Ok(Response::new(MessageResponse {
                    response: result.content,
                    success: true,
                    conversation_id: Some(conversation_id),
                }))
            }
            Err(e) => {
                tracing::error!("Failed to generate response: {}", e);
                Err(Status::internal("Failed to generate response"))
            }
        }
    }
}
