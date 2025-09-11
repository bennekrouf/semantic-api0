// src/sentence_service.rs
use crate::analyze_sentence::analyze_sentence_enhanced;
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
}

impl SentenceAnalyzeService {
    // Add a constructor to store the provider and API URL
    pub fn new(provider: Arc<dyn ModelProvider>, api_url: Option<String>) -> Self {
        Self { provider, api_url }
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

        // Use the utility function for validation
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

    #[tracing::instrument(skip(self, request), fields(client_id, email))]
    async fn analyze_sentence(
        &self,
        request: Request<SentenceRequest>,
    ) -> Result<Response<Self::AnalyzeSentenceStream>, Status> {
        let metadata = request.metadata().clone();
        let sentence_request = request.into_inner(); // Move this line up
                                                     // Log request details
        tracing::info!("Request metadata: {:?}", metadata);
        tracing::info!("Request headers: {:?}", metadata.keys());

        let client_id = Self::get_client_id(&metadata);
        // Extract email from metadata or use CLI-provided one
        let email = match self.get_email_validated(&metadata) {
            Ok(email) => email,
            Err(status) => {
                tracing::error!("Email validation failed: {}", status);
                return Err(status);
            }
        };

        let input_sentence = sentence_request.sentence;
        tracing::info!(
            input_sentence = %input_sentence,
            email = %email,
            "Processing sentence request"
        );

        // Debug logging for request details
        tracing::debug!(
            "Full request details: {:?}",
            metadata
                .iter()
                .map(|item| match item {
                    tonic::metadata::KeyAndValueRef::Ascii(k, v) =>
                        (k.as_str(), v.to_str().unwrap_or("invalid")),
                    tonic::metadata::KeyAndValueRef::Binary(k, _) => (k.as_str(), "binary value"),
                })
                .collect::<Vec<_>>()
        );

        let (tx, rx) = mpsc::channel(10);
        let analyze_span = tracing::info_span!(
            "analyze_sentence",
            client_id = %client_id,
            email = %email
        );

        // Clone the provider and API URL to move into the spawned task
        let provider_clone = self.provider.clone();
        let api_url_clone = self.api_url.clone();

        let conversation_id = sentence_request.conversation_id;
        tokio::spawn(async move {
            // Pass the input_sentence, provider, API URL, and email to analyze_sentence
            let result = analyze_sentence_enhanced(
                &input_sentence,
                provider_clone,
                api_url_clone.clone(),
                &email,
                conversation_id.clone(),
            )
            .instrument(analyze_span)
            .await;

            match result {
                Ok(enhanced_result) => {
                    tracing::info!(
                        client_id = %client_id,
                        email = %email,
                        "Analysis completed"
                    );

                    let response = SentenceResponse {
                        conversation_id: enhanced_result.conversation_id, // Add this line
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

                    tracing::info!(
                        client_id = %client_id,
                        email = %email,
                        response = ?response,
                        "Sending response"
                    );

                    if tx.send(Ok(response)).await.is_err() {
                        tracing::error!(
                            client_id = %client_id,
                            email = %email,
                            "Failed to send response - stream closed"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        input_sentence = %input_sentence,
                        error = %e,
                        client_id = %client_id,
                        email = %email,
                        "Analysis failed"
                    );

                    // Improved error handling: categorize errors for better client messages
                    let status = if e.to_string().contains("No endpoints found for user") {
                        Status::not_found(format!(
                            "No endpoints configured for your account ({}). Please contact your administrator to set up API endpoints for your account.",
                            email
                        ))
                    } else if e
                        .to_string()
                        .contains("No endpoint configuration available")
                        || e.to_string().contains("endpoints.yaml file not found")
                    {
                        Status::failed_precondition(format!(
                            "Endpoint configuration is not available. The endpoint service is not running and no local endpoints file was found. Please ensure either the endpoint service is running at {} or an endpoints.yaml file exists.",
                            api_url_clone.unwrap_or_else(|| "the configured URL".to_string())
                        ))
                    } else if e
                        .to_string()
                        .contains("Remote endpoint service is unavailable")
                    {
                        Status::unavailable(format!(
                            "The endpoint service is currently unavailable. Please try again later or contact your administrator."
                        ))
                    } else if e
                        .to_string()
                        .contains("Failed to fetch endpoints from remote service")
                    {
                        Status::internal(format!(
                            "Unable to fetch your endpoint configuration. Please check your connection or contact support."
                        ))
                    } else if e.to_string().contains("No matching endpoint found") {
                        Status::not_found(format!(
                            "No endpoint matching your input was found: {}",
                            e
                        ))
                    } else {
                        Status::internal(format!("Analysis failed: {}", e))
                    };

                    if tx.send(Err(status)).await.is_err() {
                        tracing::error!(
                            client_id = %client_id,
                            email = %email,
                            "Failed to send error response - stream closed"
                        );
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
        let conversation_id = message_request.conversation_id;

        if message.trim().is_empty() {
            return Err(Status::invalid_argument("Message cannot be empty"));
        }

        tracing::info!("Processing message: {}", message);

        // Load model configuration
        let models_config = match crate::models::config::load_models_config().await {
            Ok(config) => config,
            Err(e) => {
                tracing::error!("Failed to load models config: {}", e);
                return Err(Status::internal("Configuration error"));
            }
        };

        // Use sentence_to_json config for now, or add a specific message config
        let model_config = &models_config.sentence_to_json;

        // Call provider to generate response
        match self.provider.generate(&message, model_config).await {
            Ok(response_text) => {
                tracing::info!("Successfully generated response");
                Ok(Response::new(MessageResponse {
                    response: response_text,
                    success: true,
                    conversation_id,
                }))
            }
            Err(e) => {
                tracing::error!("Failed to generate response: {}", e);
                Err(Status::internal("Failed to generate response"))
            }
        }
    }
}
