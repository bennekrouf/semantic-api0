// src/main.rs - Updated with conversation management
mod analyze_sentence;
mod cli;
mod comparison_test;
mod conversation;
mod endpoint_client;
mod general_question_handler;
mod grpc_server;
mod help_response_handler;
mod json_helper;
mod models;
mod progressive_matching;
mod prompts;
mod sentence_service;
mod utils;
mod workflow;
use crate::models::config::load_models_config;
use crate::models::providers::{create_provider, ModelProvider, ProviderConfig};
use std::sync::Arc;
mod sentence_analysis;
use clap::Parser;
use cli::{display_custom_help, handle_cli, Cli};
use dotenv::dotenv;
use endpoint_client::get_default_api_url;
use graflog::app_log;
use graflog::init_logging;
use grpc_server::start_sentence_grpc_server;
use std::env;
use std::error::Error;
use tokio::signal;
use graflog::LogOption;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    if env::var("LOG_PATH_API0").is_err() {
        eprintln!("Error: LOG_PATH_API0 environment variable is required");
        std::process::exit(1);
    }

    let log_path = env::var("LOG_PATH_API0").unwrap_or_else(|_| "/var/log/api0.log".to_string());
    init_logging!(&log_path, "api0", "semantic", &[
        LogOption::Debug,
        LogOption::RocketOff
    ]);

    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        display_custom_help();
        std::process::exit(0);
    }

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("required arguments were not provided") {
                eprintln!("ERROR: Required arguments missing!");
                eprintln!("{error_str}");
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

    let _models_config = load_models_config().await?;

    dotenv().ok();

    let provider: Box<dyn ModelProvider> = match cli.provider.as_str() {
        "cohere" => match env::var("COHERE_API_KEY") {
            Ok(api_key) => {
                app_log!(info, "Using Cohere API");
                let config = ProviderConfig {
                    enabled: true,
                    api_key: Some(api_key),
                };
                create_provider(&config, "cohere").expect("Failed to create Cohere provider")
            }
            Err(_) => {
                app_log!(
                    error,
                    "Cohere API key not found in .env file. Please add COHERE_API_KEY to .env"
                );
                std::process::exit(1);
            }
        },
        "claude" => match env::var("CLAUDE_API_KEY") {
            Ok(api_key) => {
                app_log!(info, "Using Claude API");
                let config = ProviderConfig {
                    enabled: true,
                    api_key: Some(api_key),
                };
                create_provider(&config, "claude").expect("Failed to create Claude provider")
            }
            Err(_) => {
                app_log!(
                    error,
                    "Claude API key not found in .env file. Please add CLAUDE_API_KEY to .env"
                );
                std::process::exit(1);
            }
        },
        "deepseek" => match env::var("DEEPSEEK_API_KEY") {
            Ok(api_key) => {
                app_log!(info, "Using DeepSeek API");
                let config = ProviderConfig {
                    enabled: true,
                    api_key: Some(api_key),
                };
                create_provider(&config, "deepseek").expect("Failed to create DeepSeek provider")
            }
            Err(_) => {
                app_log!(
                    error,
                    "DeepSeek API key not found in .env file. Please add DEEPSEEK_API_KEY to .env"
                );
                std::process::exit(1);
            }
        },
        _ => {
            app_log!(
                error,
                "Invalid provider: {}. Use 'cohere' or 'claude'",
                cli.provider
            );
            std::process::exit(1);
        }
    };

    let provider_arc: Arc<dyn ModelProvider> = Arc::from(provider);

    // Get API URL from CLI or config
    let api_url = if let Some(url) = cli.api.clone() {
        Some(url)
    } else {
        // Only get default if we're not in CLI prompt mode
        if cli.prompt.is_none() && !cli.list_endpoints {
            match get_default_api_url().await {
                Ok(url) => Some(url),
                Err(e) => {
                    app_log!(error, "Failed to get default API URL: {}", e);
                    None
                }
            }
        } else {
            None
        }
    };

    // Check for CLI commands first, then default to server mode
    if cli.compare || cli.list_endpoints || cli.prompt.is_some() {
        // CLI mode - handle the command and exit
        handle_cli(cli, provider_arc).await?;
    } else {
        // Server mode - email is not needed
        app_log!(
            info,
            "No command provided, starting gRPC server with conversation management..."
        );

        let grpc_server = tokio::spawn(async move {
            if let Err(e) = start_sentence_grpc_server(provider_arc.clone(), api_url).await {
                app_log!(error, "gRPC server error: {:?}", e);
            }
        });

        app_log!(info, "Semantic server started with conversation management");

        tokio::select! {
            _ = signal::ctrl_c() => {
                app_log!(info, "Received shutdown signal, initiating graceful shutdown...");
            }
            result = grpc_server => {
                if let Err(e) = result {
                    app_log!(error, "gRPC server task error: {:?}", e);
                }
            }
        }

        app_log!(info, "Server shutting down");
    }

    Ok(())
}
