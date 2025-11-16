// src/conversation.rs
use crate::app_log;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMetadata {
    pub id: String,
    pub email: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub message_count: u32,
    pub api_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: String,
    pub conversation_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub input: String,
    pub endpoint_id: Option<String>,
    pub parameters: Option<serde_json::Value>,
}

pub struct ConversationManager {
    conversations: Arc<RwLock<HashMap<String, ConversationMetadata>>>,
    messages: Arc<RwLock<HashMap<String, Vec<ConversationMessage>>>>,
}

impl ConversationManager {
    pub fn new() -> Self {
        Self {
            conversations: Arc::new(RwLock::new(HashMap::new())),
            messages: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start_conversation(
        &self,
        email: String,
        api_url: Option<String>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let conversation_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        let metadata = ConversationMetadata {
            id: conversation_id.clone(),
            email,
            created_at: now,
            last_activity: now,
            message_count: 0,
            api_url,
        };

        {
            let mut conversations = self.conversations.write().await;
            conversations.insert(conversation_id.clone(), metadata);
        }

        {
            let mut messages = self.messages.write().await;
            messages.insert(conversation_id.clone(), Vec::new());
        }

        app_log!(info, "Started new conversation: {}", conversation_id);
        Ok(conversation_id)
    }

    pub async fn add_message(
        &self,
        conversation_id: &str,
        input: String,
        endpoint_id: Option<String>,
        parameters: Option<serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let message_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        let message = ConversationMessage {
            id: message_id,
            conversation_id: conversation_id.to_string(),
            timestamp: now,
            input,
            endpoint_id,
            parameters,
        };

        // Update conversation metadata
        {
            let mut conversations = self.conversations.write().await;
            if let Some(metadata) = conversations.get_mut(conversation_id) {
                metadata.last_activity = now;
                metadata.message_count += 1;
            } else {
                return Err(format!("Conversation {conversation_id} not found").into());
            }
        }

        // Add message
        {
            let mut messages = self.messages.write().await;
            if let Some(conversation_messages) = messages.get_mut(conversation_id) {
                conversation_messages.push(message);
            } else {
                return Err(format!("Conversation {conversation_id} not found").into());
            }
        }

        app_log!(debug, "Added message to conversation: {}", conversation_id);
        Ok(())
    }
}

impl Default for ConversationManager {
    fn default() -> Self {
        Self::new()
    }
}
