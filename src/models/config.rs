// src/models/config.rs
use crate::models::ModelsConfig;
use serde::Deserialize;
use std::error::Error;
use tracing::debug;

use std::env;

fn get_config_path() -> String {
    env::var("CONFIG_PATH").unwrap_or_else(|_| "config.yaml".to_string())
}

#[derive(Debug, Deserialize, Clone)]
pub struct Providers {}

#[derive(Debug, Deserialize, Clone)]
pub struct GrpcConfig {}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EndpointClientConfig {
    pub default_address: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub models: ModelsConfig,
    pub server: ServerConfig,
    pub endpoint_client: EndpointClientConfig,
    // pub debug_mode: Option<DebugConfig>, // Optional to maintain backward compatibility
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct DebugConfig {
    // pub enabled: bool,
    // pub use_local_endpoints: bool,
}

pub async fn load_models_config() -> Result<ModelsConfig, Box<dyn Error + Send + Sync>> {
    let config_path = get_config_path();
    let config_str = tokio::fs::read_to_string(&config_path).await?;
    let config: Config = serde_yaml::from_str(&config_str)?;

    debug!("Loaded models configuration from: {}", config_path);
    debug!("Models config: {:#?}", config.models);

    Ok(config.models)
}

// Load server configuration from config file
pub async fn load_server_config() -> Result<ServerConfig, Box<dyn Error + Send + Sync>> {
    let config_path = get_config_path();
    let config_str = tokio::fs::read_to_string(&config_path).await?;
    let config: Config = serde_yaml::from_str(&config_str)?;

    debug!("Loaded server configuration from: {}", config_path);
    debug!("Server config: {:#?}", config.server);

    Ok(config.server)
}

// Load endpoint client configuration from config file
pub async fn load_endpoint_client_config(
) -> Result<EndpointClientConfig, Box<dyn Error + Send + Sync>> {
    let config_path = get_config_path();
    let config_str = tokio::fs::read_to_string(&config_path).await?;
    let config: Config = serde_yaml::from_str(&config_str)?;

    debug!("Loaded endpoint client configuration from: {}", config_path);
    debug!("Endpoint client config: {:#?}", config.endpoint_client);

    Ok(config.endpoint_client)
}
