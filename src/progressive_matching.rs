// src/progressive_matching.rs - PostgreSQL implementation
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use tracing::{debug, info};

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use tokio_postgres::Config as PgConfig;
use tokio_postgres::NoTls;

async fn create_db_pool(database_url: &str) -> Result<Pool, Box<dyn Error + Send + Sync>> {
    // Parse the PostgreSQL connection string directly
    let pg_config: PgConfig = database_url.parse()?;

    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let mgr = Manager::new(pg_config, NoTls);
    let pool = Pool::builder(mgr)
        .max_size(10)
        .runtime(deadpool_postgres::Runtime::Tokio1)
        .build()?;

    Ok(pool)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OngoingMatch {
    pub conversation_id: String,
    pub endpoint_id: String,
    pub parameters: String,
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
    pool: Pool,
}

impl ProgressiveMatchingManager {
    pub async fn new(database_url: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let pool = create_db_pool(database_url).await?;

        // Initialize database schema
        let client = pool.get().await?;
        client
            .execute(
                r#"
                CREATE TABLE IF NOT EXISTS ongoing_matches (
                    conversation_id TEXT NOT NULL,
                    endpoint_id TEXT NOT NULL,
                    parameters TEXT NOT NULL,
                    completion_percentage REAL NOT NULL DEFAULT 0.0,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY (conversation_id, endpoint_id)
                )
                "#,
                &[],
            )
            .await?;

        Ok(Self { pool })
    }

    pub async fn update_match(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
        new_parameters: Vec<ParameterValue>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let now = chrono::Utc::now().to_rfc3339();
        let client = self.pool.get().await?;

        // Get existing parameters
        let existing_params: Option<String> = client
            .query_opt(
                "SELECT parameters FROM ongoing_matches WHERE conversation_id = $1 AND endpoint_id = $2",
                &[&conversation_id, &endpoint_id],
            )
            .await?
            .map(|row| row.get(0));

        // Merge parameters
        let mut all_parameters = if let Some(existing_json) = existing_params {
            serde_json::from_str::<Vec<ParameterValue>>(&existing_json)?
        } else {
            Vec::new()
        };

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

        // Get existing created_at or use current time
        let created_at: String = client
            .query_opt(
                "SELECT created_at FROM ongoing_matches WHERE conversation_id = $1 AND endpoint_id = $2",
                &[&conversation_id, &endpoint_id],
            )
            .await?
            .map(|row| row.get(0))
            .unwrap_or_else(|| now.clone());

        client
            .execute(
                r#"
                INSERT INTO ongoing_matches 
                (conversation_id, endpoint_id, parameters, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (conversation_id, endpoint_id) 
                DO UPDATE SET parameters = $3, updated_at = $5
                "#,
                &[
                    &conversation_id,
                    &endpoint_id,
                    &parameters_json,
                    &created_at,
                    &now,
                ],
            )
            .await?;

        info!(
            "Updated progressive match for conversation: {} endpoint: {}",
            conversation_id, endpoint_id
        );
        Ok(())
    }

    pub async fn get_incomplete_match(
        &self,
        conversation_id: &str,
    ) -> Result<Option<OngoingMatch>, Box<dyn Error + Send + Sync>> {
        let client = self.pool.get().await?;

        let result = client
            .query_opt(
                "SELECT conversation_id, endpoint_id, parameters, created_at, updated_at 
                 FROM ongoing_matches 
                 WHERE conversation_id = $1
                 LIMIT 1",
                &[&conversation_id],
            )
            .await?
            .map(|row| OngoingMatch {
                conversation_id: row.get(0),
                endpoint_id: row.get(1),
                parameters: row.get(2),
                created_at: row.get(3),
                updated_at: row.get(4),
            });

        Ok(result)
    }

    pub async fn complete_match(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let client = self.pool.get().await?;

        client
            .execute(
                "DELETE FROM ongoing_matches WHERE conversation_id = $1 AND endpoint_id = $2",
                &[&conversation_id, &endpoint_id],
            )
            .await?;

        info!(
            "Completed and cleaned up match for conversation: {}",
            conversation_id
        );
        Ok(())
    }

    pub async fn get_match(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
    ) -> Result<Option<OngoingMatch>, Box<dyn Error + Send + Sync>> {
        let client = self.pool.get().await?;

        let result = client
            .query_opt(
                "SELECT conversation_id, endpoint_id, parameters, created_at, updated_at 
                 FROM ongoing_matches 
                 WHERE conversation_id = $1 AND endpoint_id = $2",
                &[&conversation_id, &endpoint_id],
            )
            .await?
            .map(|row| OngoingMatch {
                conversation_id: row.get(0),
                endpoint_id: row.get(1),
                parameters: row.get(2),
                created_at: row.get(3),
                updated_at: row.get(4),
            });

        Ok(result)
    }

    pub async fn check_completion(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
        required_parameters: Vec<String>,
        endpoint_parameters: &[crate::models::EndpointParameter],
    ) -> Result<ProgressiveMatchResult, Box<dyn Error + Send + Sync>> {
        let ongoing_match = self.get_match(conversation_id, endpoint_id).await?;

        let matched_parameters = if let Some(match_data) = ongoing_match {
            serde_json::from_str::<Vec<ParameterValue>>(&match_data.parameters)?
        } else {
            Vec::new()
        };

        // Generic parameter matching using endpoint definitions
        let mut satisfied_required_params = Vec::new();
        let mut missing_parameters = Vec::new();

        for required_param in &required_parameters {
            let is_satisfied =
                is_parameter_satisfied(required_param, &matched_parameters, endpoint_parameters);

            if is_satisfied {
                satisfied_required_params.push(required_param.clone());
            } else {
                missing_parameters.push(required_param.clone());
            }
        }

        let is_complete = missing_parameters.is_empty();
        let completion_percentage = if required_parameters.is_empty() {
            100.0
        } else {
            (satisfied_required_params.len() as f32 / required_parameters.len() as f32) * 100.0
        };

        Ok(ProgressiveMatchResult {
            conversation_id: conversation_id.to_string(),
            endpoint_id: endpoint_id.to_string(),
            endpoint_description: format!("Endpoint {endpoint_id}"),
            matched_parameters,
            missing_parameters,
            is_complete,
            completion_percentage,
            ready_for_execution: is_complete,
        })
    }
}

// Generic parameter satisfaction checker
fn is_parameter_satisfied(
    required_param: &str,
    matched_parameters: &[ParameterValue],
    endpoint_parameters: &[crate::models::EndpointParameter],
) -> bool {
    // Find the endpoint parameter definition
    let endpoint_param = endpoint_parameters
        .iter()
        .find(|p| p.name == required_param);

    for matched in matched_parameters {
        // Direct match
        if matched.name == required_param {
            return true;
        }

        // Check alternatives from endpoint definition
        if let Some(ep) = endpoint_param {
            if let Some(ref alternatives) = ep.alternatives {
                if alternatives.contains(&matched.name) {
                    return true;
                }
            }
        }

        // Reverse check: see if the matched parameter accepts this required param as alternative
        if let Some(matched_endpoint_param) =
            endpoint_parameters.iter().find(|p| p.name == matched.name)
        {
            if let Some(ref alternatives) = matched_endpoint_param.alternatives {
                if alternatives.contains(&required_param.to_string()) {
                    return true;
                }
            }
        }
    }

    false
}

pub async fn integrate_progressive_matching(
    conversation_id: &str,
    endpoint_id: &str,
    new_parameters: Vec<ParameterValue>,
    required_parameter_names: Vec<String>,
    manager: &ProgressiveMatchingManager,
    endpoint_parameters: &[crate::models::EndpointParameter],
) -> Result<ProgressiveMatchResult, Box<dyn Error + Send + Sync>> {
    manager
        .update_match(conversation_id, endpoint_id, new_parameters)
        .await?;
    let result = manager
        .check_completion(
            conversation_id,
            endpoint_id,
            required_parameter_names,
            endpoint_parameters,
        )
        .await?;

    debug!(
        "Progressive matching result: completion {}%, ready: {}",
        result.completion_percentage, result.ready_for_execution
    );

    Ok(result)
}

pub fn get_database_url() -> Result<String, Box<dyn Error + Send + Sync>> {
    env::var("DATABASE_URL").map_err(|_| "DATABASE_URL environment variable not set".into())
}
