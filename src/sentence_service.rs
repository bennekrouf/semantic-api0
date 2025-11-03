// src/sentence_service.rs
use crate::conversation::ConversationManager;
use crate::models::providers::ModelProvider;
use crate::progressive_matching::ProgressiveMatchingManager;
use crate::sentence_analysis::SentenceAnalyzer;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};

pub mod sentence {
    tonic::include_proto!("sentence");
}

use crate::app_log;
use sentence::sentence_service_server::SentenceService;
use sentence::{MessageRequest, MessageResponse, SentenceRequest, SentenceResponse};
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
pub struct SentenceAnalyzeService {
    analyzer: SentenceAnalyzer,
}

impl SentenceAnalyzeService {
    pub fn new(provider: Arc<dyn ModelProvider>, api_url: Option<String>) -> Self {
        let analyzer = SentenceAnalyzer::new(
            provider,
            api_url,
            Arc::new(ConversationManager::new()),
            None,
        );
        Self { analyzer }
    }

    pub async fn with_progressive_matching(
        provider: Arc<dyn ModelProvider>,
        api_url: Option<String>,
        database_url: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let progressive_manager = Arc::new(ProgressiveMatchingManager::new(database_url).await?);
        let analyzer = SentenceAnalyzer::new(
            provider,
            api_url,
            Arc::new(ConversationManager::new()),
            Some(progressive_manager),
        );
        Ok(Self { analyzer })
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
                "Email validation failed: {e}"
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
                // Always use the provided conversation ID - don't check if it exists
                Ok(id)
            }
            _ => {
                self.analyzer
                    .conversation_manager
                    .start_conversation(email.to_string(), self.analyzer.api_url.clone())
                    .await
            }
        }
    }
}

impl std::fmt::Debug for SentenceAnalyzeService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SentenceAnalyzeService")
            .field("analyzer", &"<SentenceAnalyzer>")
            .finish()
    }
}

#[tonic::async_trait]
impl SentenceService for SentenceAnalyzeService {
    type AnalyzeSentenceStream =
        Pin<Box<dyn Stream<Item = Result<SentenceResponse, Status>> + Send>>;

    async fn analyze_sentence(
        &self,
        request: Request<SentenceRequest>,
    ) -> Result<Response<Self::AnalyzeSentenceStream>, Status> {
        let metadata = request.metadata().clone();
        let sentence_request = request.into_inner();

        app_log!(info, "Request metadata: {:?}", metadata);

        let client_id = Self::get_client_id(&metadata);
        let email = match self.get_email_validated(&metadata) {
            Ok(email) => email,
            Err(status) => {
                app_log!(error, "Email validation failed: {}", status);
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
                app_log!(error, "Failed to ensure conversation_id: {}", e);
                return Err(Status::internal("Failed to manage conversation"));
            }
        };

        app_log!(info,
            input_sentence = %input_sentence,
            email = %email,
            conversation_id = %conversation_id,
            "Processing sentence request"
        );

        let (tx, rx) = mpsc::channel(10);

        let analyzer = self.analyzer.clone();
        tokio::spawn(async move {
            analyzer
                .analyze_sentence_stream(input_sentence, conversation_id, email, client_id, tx)
                .await;
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

        app_log!(info,
            message = %message,
            conversation_id = %conversation_id,
            "Processing message"
        );

        let models_config = match crate::models::config::load_models_config().await {
            Ok(config) => config,
            Err(e) => {
                app_log!(error, "Failed to load models config: {}", e);
                return Err(Status::internal("Configuration error"));
            }
        };

        let model_config = &models_config.default;

        match self
            .analyzer
            .provider
            .generate(&message, model_config)
            .await
        {
            Ok(result) => {
                app_log!(info, "Successfully generated response");
                Ok(Response::new(MessageResponse {
                    response: result.content,
                    success: true,
                    conversation_id: Some(conversation_id),
                }))
            }
            Err(e) => {
                app_log!(error, "Failed to generate response: {}", e);
                Err(Status::internal("Failed to generate response"))
            }
        }
    }
}
