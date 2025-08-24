// src/main.rs
mod analyze_sentence;
mod cli;
mod endpoint_client;
mod grpc_server;
mod json_helper;
mod models;
mod prompts;
mod sentence_service;
mod utils;
mod workflow;

use crate::models::config::load_models_config;
use crate::models::providers::{create_provider, ModelProvider, ProviderConfig};
use std::sync::Arc;

use clap::Parser;
use cli::{display_custom_help, handle_cli, Cli};
use dotenv::dotenv;
use endpoint_client::get_default_api_url;
use grpc_logger::load_config;
use grpc_server::start_sentence_grpc_server;
use std::env;
use std::error::Error;
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let _log_config = load_config("config.yaml")?;

    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        // No arguments provided - show custom help
        display_custom_help();
        std::process::exit(0);
    }

    Registry::default()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("INFO")))
        .init();

    // Parse CLI arguments
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let error_str = e.to_string();

            if error_str.contains("required arguments were not provided") {
                eprintln!("ERROR: Required arguments missing!");
                eprintln!("{}", error_str);
                eprintln!("\nNOTE: When analyzing a sentence, email is required:");
                eprintln!("  --email user@example.com");
                eprintln!("\nExample for starting server (no email needed):");
                eprintln!("  cargo run");
                eprintln!("\nExample for analyzing a sentence (email required):");
                eprintln!("  cargo run -- --email user@example.com \"analyze this text\"");

                std::process::exit(1);
            } else {
                e.exit();
            }
        }
    };

    // Load model configuration
    let _models_config = load_models_config().await?;

    // Initialize Cohere provider
    dotenv().ok();
    let provider: Box<dyn ModelProvider> = match env::var("COHERE_API_KEY") {
        Ok(api_key) => {
            info!("Using Cohere API");
            let config = ProviderConfig {
                enabled: true,
                api_key: Some(api_key),
            };
            create_provider(&config).expect("Failed to create Cohere provider")
        }
        Err(_) => {
            error!("Cohere API key not found in .env file. Please add COHERE_API_KEY to .env");
            std::process::exit(1);
        }
    };

    // Wrap the provider in an Arc so we can clone it
    let provider_arc: Arc<dyn ModelProvider> = Arc::from(provider);

    // Get API URL from CLI or config
    let api_url = if let Some(url) = cli.api.clone() {
        Some(url)
    } else {
        // Only get default if we're not in CLI prompt mode
        if cli.prompt.is_none() {
            match get_default_api_url().await {
                Ok(url) => Some(url),
                Err(e) => {
                    error!("Failed to get default API URL: {}", e);
                    None
                }
            }
        } else {
            None
        }
    };

    // Handle CLI command if present, otherwise start gRPC server
    match cli.prompt {
        Some(_) => {
            // CLI mode with a prompt - email is required and validated in handle_cli
            handle_cli(cli, provider_arc).await?;
        }
        None => {
            // Server mode - email is not needed
            info!("No prompt provided, starting gRPC server with Cohere API...");

            // Start the gRPC server with our API URL if provided
            let grpc_server = tokio::spawn(async move {
                if let Err(e) = start_sentence_grpc_server(provider_arc.clone(), api_url).await {
                    error!("gRPC server error: {:?}", e);
                }
            });

            info!("Semantic server started");

            // Wait for CTRL-C
            tokio::select! {
                _ = signal::ctrl_c() => {
                    info!("Received shutdown signal, initiating graceful shutdown...");
                }
                result = grpc_server => {
                    if let Err(e) = result {
                        error!("gRPC server task error: {:?}", e);
                    }
                }
            }

            info!("Server shutting down");
        }
    }

    Ok(())
}

