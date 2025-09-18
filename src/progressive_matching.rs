// src/progressive_matching.rs - Using custom SQLite connection manager
use mobc::{Manager, Pool};
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info};

pub type MobcSQLitePool = Pool<SQLiteConnectionManager>;
// pub type MobcSQLiteConnection = MobcConnection<SQLiteConnectionManager>;

#[derive(Debug, Error)]
pub enum DbPoolError {
    #[error("SQLite error: {0}")]
    SQLiteError(#[from] rusqlite::Error),
    #[error("Pool error: {0:?}")]
    PoolError(mobc::Error<SQLiteConnectionManager>),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl From<mobc::Error<SQLiteConnectionManager>> for DbPoolError {
    fn from(err: mobc::Error<SQLiteConnectionManager>) -> Self {
        DbPoolError::PoolError(err)
    }
}

#[derive(Clone, Debug)]
pub struct SQLiteConnectionManager {
    db_path: Arc<String>,
}

impl SQLiteConnectionManager {
    pub fn file<P: AsRef<Path>>(path: P) -> Result<Self, DbPoolError> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let path_str = path.as_ref().to_string_lossy().to_string();
        Ok(Self {
            db_path: Arc::new(path_str),
        })
    }
}

#[async_trait::async_trait]
impl Manager for SQLiteConnectionManager {
    type Connection = Connection;
    type Error = rusqlite::Error;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let conn = Connection::open(self.db_path.as_str())?;
        conn.execute("PRAGMA foreign_keys=ON", [])?;
        Ok(conn)
    }

    async fn check(&self, conn: Self::Connection) -> Result<Self::Connection, Self::Error> {
        conn.execute("SELECT 1", [])?;
        Ok(conn)
    }
}

pub fn create_db_pool<P: AsRef<Path>>(
    db_path: P,
    max_pool_size: u64,
    max_idle_timeout: Option<Duration>,
) -> Result<MobcSQLitePool, DbPoolError> {
    let manager = SQLiteConnectionManager::file(db_path)?;
    let mut builder = Pool::builder().max_open(max_pool_size);
    if let Some(timeout) = max_idle_timeout {
        builder = builder.max_idle_lifetime(Some(timeout));
    }
    Ok(builder.build(manager))
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
    pool: MobcSQLitePool,
}

impl ProgressiveMatchingManager {
    pub async fn new(database_path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let db_path = database_path
            .strip_prefix("sqlite:")
            .unwrap_or(database_path);

        let pool = create_db_pool(db_path, 10, Some(Duration::from_secs(300)))?;

        // Initialize database schema
        let conn = pool.get().await?;
        conn.execute(
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
            params![],
        )?;

        Ok(Self { pool })
    }

    pub async fn update_match(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
        new_parameters: Vec<ParameterValue>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.pool.get().await?;

        // Get existing parameters
        let existing_params: Option<String> = conn
            .query_row(
                "SELECT parameters FROM ongoing_matches WHERE conversation_id = ? AND endpoint_id = ?",
                params![conversation_id, endpoint_id],
                |row| row.get(0),
            )
            .optional()?;

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
        let created_at: String = conn
            .query_row(
                "SELECT created_at FROM ongoing_matches WHERE conversation_id = ? AND endpoint_id = ?",
                params![conversation_id, endpoint_id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| now.clone());

        conn.execute(
            r#"
            INSERT OR REPLACE INTO ongoing_matches 
            (conversation_id, endpoint_id, parameters, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
            params![
                conversation_id,
                endpoint_id,
                parameters_json,
                created_at,
                now
            ],
        )?;

        info!(
            "Updated progressive match for conversation: {} endpoint: {}",
            conversation_id, endpoint_id
        );
        Ok(())
    }

    pub async fn get_match(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
    ) -> Result<Option<OngoingMatch>, Box<dyn Error + Send + Sync>> {
        let conn = self.pool.get().await?;

        let result = conn
            .query_row(
                "SELECT conversation_id, endpoint_id, parameters, created_at, updated_at FROM ongoing_matches WHERE conversation_id = ? AND endpoint_id = ?",
                params![conversation_id, endpoint_id],
                |row| {
                    Ok(OngoingMatch {
                        conversation_id: row.get(0)?,
                        endpoint_id: row.get(1)?,
                        parameters: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()?;

        Ok(result)
    }

    pub async fn check_completion(
        &self,
        conversation_id: &str,
        endpoint_id: &str,
        required_parameters: Vec<String>,
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
            endpoint_description: format!("Endpoint {}", endpoint_id),
            matched_parameters,
            missing_parameters,
            is_complete,
            completion_percentage,
            ready_for_execution: is_complete,
        })
    }
}

pub async fn integrate_progressive_matching(
    conversation_id: &str,
    endpoint_id: &str,
    new_parameters: Vec<ParameterValue>,
    required_parameter_names: Vec<String>,
    manager: &ProgressiveMatchingManager,
) -> Result<ProgressiveMatchResult, Box<dyn Error + Send + Sync>> {
    manager
        .update_match(conversation_id, endpoint_id, new_parameters)
        .await?;
    let result = manager
        .check_completion(conversation_id, endpoint_id, required_parameter_names)
        .await?;

    debug!(
        "Progressive matching result: completion {}%, ready: {}",
        result.completion_percentage, result.ready_for_execution
    );

    Ok(result)
}

pub fn get_database_url() -> Result<String, Box<dyn Error + Send + Sync>> {
    if let Ok(url) = env::var("DATABASE_URL") {
        return Ok(url);
    }

    let db_path_str = env::var("DB_PATH").unwrap_or_else(|_| "./data".to_string());
    let db_dir = PathBuf::from(&db_path_str);

    // Debug info
    println!(
        "Creating database at: {}",
        format!("sqlite:{}/conversations.db", db_dir.display())
    );
    println!("Current working directory: {:?}", std::env::current_dir());
    println!("Database directory exists: {}", db_dir.exists());
    println!(
        "Database directory permissions: {:?}",
        std::fs::metadata(&db_dir)
    );

    if !db_dir.exists() {
        match std::fs::create_dir_all(&db_dir) {
            Ok(_) => println!("Successfully created directory: {}", db_dir.display()),
            Err(e) => {
                println!("Failed to create directory {}: {}", db_dir.display(), e);
                return Err(format!("Cannot create database directory: {}", e).into());
            }
        }
    }

    let db_file = db_dir.join("conversations.db");

    // Test if we can create/write to the database file
    match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&db_file)
    {
        Ok(_) => println!("Successfully tested database file access"),
        Err(e) => {
            println!("Cannot access database file {}: {}", db_file.display(), e);
            return Err(format!("Cannot access database file: {}", e).into());
        }
    }

    Ok(format!("sqlite:{}", db_file.display()))
}
