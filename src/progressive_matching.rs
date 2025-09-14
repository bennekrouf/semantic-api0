// src/progressive_matching.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::error::Error;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OngoingMatch {
    pub conversation_id: String,
    pub endpoint_id: String,
    pub parameters: String, // JSON string of matched parameters
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterValue {
    pub name: String,
    pub value: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressiveMatchResult {
    pub conversation_id: String,
    pub endpoint_id: String,
    pub endpoint_description: String,
    pub matched_parameters: Vec<ParameterValue>,
    pub missing_parameters: Vec<String>,
    pub is_complete: bool,
    pub completion_percentage: f32,
    pub ready_for_execution: bool,
}

pub struct ProgressiveMatchingManager {
    pool: SqlitePool,
}

impl ProgressiveMatchingManager {
    pub async fn new(database_url: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        println!("Creating database at: {}", database_url);
        println!("Current working directory: {:?}", std::env::current_dir());
        let pool = SqlitePool::connect(database_url).await?;

        // Create table if it doesn't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ongoing_matches (
                conversation_id TEXT NOT NULL,
                endpoint_id TEXT NOT NULL,
                parameters TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (conversation_id, endpoint_id)
            )
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    // Store or update matched parameters for a conversation/endpoint
    pub async fn update_match(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
        new_parameters: Vec<ParameterValue>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let now = chrono::Utc::now().to_rfc3339();

        // Get existing parameters if any
        let existing = self.get_match(conversation_id, endpoint_id).await?;

        let mut all_parameters = if let Some(existing_match) = existing {
            // Parse existing parameters
            serde_json::from_str::<Vec<ParameterValue>>(&existing_match.parameters)?
        } else {
            Vec::new()
        };

        // Merge new parameters (update existing or add new)
        for new_param in new_parameters {
            if let Some(existing_param) =
                all_parameters.iter_mut().find(|p| p.name == new_param.name)
            {
                existing_param.value = new_param.value;
            } else {
                all_parameters.push(new_param);
            }
        }

        let parameters_json = serde_json::to_string(&all_parameters)?;

        // Upsert the record
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO ongoing_matches 
            (conversation_id, endpoint_id, parameters, created_at, updated_at)
            VALUES (?, ?, ?, COALESCE((SELECT created_at FROM ongoing_matches WHERE conversation_id = ? AND endpoint_id = ?), ?), ?)
            "#
        )
        .bind(conversation_id)
        .bind(endpoint_id)
        .bind(&parameters_json)
        .bind(conversation_id)
        .bind(endpoint_id)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        info!(
            "Updated progressive match for conversation: {} endpoint: {}",
            conversation_id, endpoint_id
        );
        Ok(())
    }

    // Get current match state for a conversation/endpoint
    pub async fn get_match(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
    ) -> Result<Option<OngoingMatch>, Box<dyn Error + Send + Sync>> {
        let result = sqlx::query_as::<_, OngoingMatch>(
            "SELECT * FROM ongoing_matches WHERE conversation_id = ? AND endpoint_id = ?",
        )
        .bind(conversation_id)
        .bind(endpoint_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    // Check if endpoint is fully matched and ready for execution
    pub async fn check_completion(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
        required_parameters: Vec<String>, // Parameter names that are required
    ) -> Result<ProgressiveMatchResult, Box<dyn Error + Send + Sync>> {
        let ongoing_match = self.get_match(conversation_id, endpoint_id).await?;

        let matched_parameters = if let Some(match_data) = ongoing_match {
            serde_json::from_str::<Vec<ParameterValue>>(&match_data.parameters)?
        } else {
            Vec::new()
        };

        let matched_names: Vec<String> =
            matched_parameters.iter().map(|p| p.name.clone()).collect();
        let missing_parameters: Vec<String> = required_parameters
            .iter()
            .filter(|req| !matched_names.contains(req))
            .cloned()
            .collect();

        let is_complete = missing_parameters.is_empty();
        let completion_percentage = if required_parameters.is_empty() {
            100.0
        } else {
            (matched_names.len() as f32 / required_parameters.len() as f32) * 100.0
        };

        Ok(ProgressiveMatchResult {
            conversation_id: conversation_id.to_string(),
            endpoint_id: endpoint_id.to_string(),
            endpoint_description: format!("Endpoint {}", endpoint_id), // You can enhance this
            matched_parameters,
            missing_parameters,
            is_complete,
            completion_percentage,
            ready_for_execution: is_complete,
        })
    }

    // Clear ongoing match after successful execution
    pub async fn clear_match(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query("DELETE FROM ongoing_matches WHERE conversation_id = ? AND endpoint_id = ?")
            .bind(conversation_id)
            .bind(endpoint_id)
            .execute(&self.pool)
            .await?;

        info!(
            "Cleared progressive match for conversation: {} endpoint: {}",
            conversation_id, endpoint_id
        );
        Ok(())
    }

    // Get all ongoing matches for a conversation
    pub async fn get_all_matches(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<OngoingMatch>, Box<dyn Error + Send + Sync>> {
        let results = sqlx::query_as::<_, OngoingMatch>(
            "SELECT * FROM ongoing_matches WHERE conversation_id = ? ORDER BY updated_at DESC",
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(results)
    }

    // Cleanup old matches (optional maintenance)
    pub async fn cleanup_old_matches(
        &self,
        days_old: i64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days_old);
        let cutoff_str = cutoff.to_rfc3339();

        let deleted = sqlx::query("DELETE FROM ongoing_matches WHERE updated_at < ?")
            .bind(cutoff_str)
            .execute(&self.pool)
            .await?;

        info!(
            "Cleaned up {} old progressive matches",
            deleted.rows_affected()
        );
        Ok(())
    }
}

// Integration with your existing analyze function
pub async fn integrate_progressive_matching(
    conversation_id: &str,
    endpoint_id: &str,
    new_parameters: Vec<ParameterValue>,
    required_parameter_names: Vec<String>,
    manager: &ProgressiveMatchingManager,
) -> Result<ProgressiveMatchResult, Box<dyn Error + Send + Sync>> {
    // Update the ongoing match with new parameters
    manager
        .update_match(conversation_id, endpoint_id, new_parameters)
        .await?;

    // Check completion status
    let result = manager
        .check_completion(conversation_id, endpoint_id, required_parameter_names)
        .await?;

    debug!(
        "Progressive matching result: completion {}%, ready: {}",
        result.completion_percentage, result.ready_for_execution
    );

    Ok(result)
}
