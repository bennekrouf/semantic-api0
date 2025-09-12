// src/sentence_service.rs - Updated with conversation management
use crate::analyze_sentence::analyze_sentence_enhanced;
use crate::conversation::{
    ConversationManager, StartConversationRequest, StartConversationResponse,
};
use crate::models::providers::ModelProvider;
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
}

impl SentenceAnalyzeService {
    pub fn new(provider: Arc<dyn ModelProvider>, api_url: Option<String>) -> Self {
        Self {
            provider,
            api_url,
            conversation_manager: Arc::new(ConversationManager::new()),
        }
    }

    // Get email from metadata with validation
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

    // Helper function to extract client_id from metadata
    fn get_client_id(metadata: &MetadataMap) -> String {
        metadata
            .get("client-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown-client")
            .to_string()
    }

    // New method to start conversations
    pub async fn start_conversation(
        &self,
        request: StartConversationRequest,
    ) -> Result<StartConversationResponse, Box<dyn std::error::Error + Send + Sync>> {
        match crate::utils::email::validate_email(&request.email) {
            Ok(_) => {}
            Err(e) => {
                return Ok(StartConversationResponse {
                    conversation_id: String::new(),
                    success: false,
                    message: format!("Invalid email: {}", e),
                });
            }
        }

        match self
            .conversation_manager
            .start_conversation(request.email.clone(), request.api_url.clone())
            .await
        {
            Ok(conversation_id) => Ok(StartConversationResponse {
                conversation_id,
                success: true,
                message: "Conversation started successfully".to_string(),
            }),
            Err(e) => Ok(StartConversationResponse {
                conversation_id: String::new(),
                success: false,
                message: format!("Failed to start conversation: {}", e),
            }),
        }
    }

    // Helper to generate conversation_id if not provided
    async fn ensure_conversation_id(
        &self,
        conversation_id: Option<String>,
        email: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        match conversation_id {
            Some(id) if !id.is_empty() => {
                // Verify conversation exists
                if self
                    .conversation_manager
                    .get_conversation(&id)
                    .await
                    .is_some()
                {
                    Ok(id)
                } else {
                    // Conversation doesn't exist, create a new one
                    tracing::warn!("Conversation {} not found, creating new one", id);
                    self.conversation_manager
                        .start_conversation(email.to_string(), self.api_url.clone())
                        .await
                }
            }
            _ => {
                // No conversation_id provided, create new one
                self.conversation_manager
                    .start_conversation(email.to_string(), self.api_url.clone())
                    .await
            }
        }
    }
}

// Implement Debug manually
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

        // Ensure we have a valid conversation_id
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

        // Clone necessary data for the spawned task
        let provider_clone = self.provider.clone();
        let api_url_clone = self.api_url.clone();
        let conversation_manager_clone = self.conversation_manager.clone();
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
                        "Analysis completed"
                    );

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
                            missing_required_fields: enhanced_result
                                .matching_info
                                .missing_required_fields
                                .into_iter()
                                .map(|field| sentence::MissingField {
                                    name: field.name,
                                    description: field.description,
                                })
                                .collect(),
                            missing_optional_fields: enhanced_result
                                .matching_info
                                .missing_optional_fields
                                .into_iter()
                                .map(|field| sentence::MissingField {
                                    name: field.name,
                                    description: field.description,
                                })
                                .collect(),
                        }),
                    };

                    if tx.send(Ok(response)).await.is_err() {
                        tracing::error!(
                            client_id = %client_id,
                            email = %email_clone,
                            conversation_id = %conversation_id_clone.clone(),
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

        // Get conversation_id or create new one
        let conversation_id = message_request
            .conversation_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        tracing::info!(
            message = %message,
            conversation_id = %conversation_id,
            "Processing message"
        );

        // Load model configuration
        let models_config = match crate::models::config::load_models_config().await {
            Ok(config) => config,
            Err(e) => {
                tracing::error!("Failed to load models config: {}", e);
                return Err(Status::internal("Configuration error"));
            }
        };

        let model_config = &models_config.sentence_to_json;

        // Generate response using the provider
        match self.provider.generate(&message, model_config).await {
            Ok(response_text) => {
                tracing::info!("Successfully generated response");
                Ok(Response::new(MessageResponse {
                    response: response_text,
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
