// src/cli.rs - Updated to use only Cohere
use clap::Parser;
use std::{error::Error, sync::Arc};
use tracing::{error, info};

use crate::endpoint_client::get_default_api_url;
use crate::utils::email::validate_email;
use crate::{analyze_sentence::analyze_sentence_enhanced, models::providers::ModelProvider};

pub fn display_custom_help() {
    println!(
        "
╭─────────────────────────────────────────────╮
│                  Semantic                      │
│         Natural Language API Matcher           │
╰─────────────────────────────────────────────╯

ARGUMENTS:
  --email ADDRESS    Your email address 
                     (REQUIRED ONLY when analyzing a sentence)

  --api URL          Remote API endpoint for fetching endpoints
                     Default: Uses local endpoints.yaml
  
  --port PORT        Override gRPC server port
                     Default: From config.yaml

USAGE EXAMPLES:
  1. Start gRPC server (no email required):
     semantic
  
  2. Analyze text (email required):
     semantic --email user@example.com \"analyze this text\"
  
  3. Use remote endpoints:
     semantic --api http://example.com:50053 --email user@example.com \"analyze this\"

For more information, use the standard help:
  semantic --help
"
    );
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(help_template = "\
{before-help}{name} {version}
{author}
{about}

[REQUIRED PARAMETERS]
--email ADDRESS    : valid email address (only when analyzing a sentence)

{usage-heading} {usage}

{all-args}{after-help}
")]
pub struct Cli {
    #[arg(long, value_name = "PROVIDER", default_value = "cohere")]
    pub provider: String,
    /// The sentence to analyze (if not provided, starts gRPC server)
    pub prompt: Option<String>,

    /// Remote API endpoint for fetching endpoint definitions (optional)
    #[arg(long, value_name = "URL")]
    pub api: Option<String>,

    /// Email address for authentication (required when analyzing a sentence)
    #[arg(
        long,
        value_name = "EMAIL",
        help = "Email address (required when analyzing a sentence)"
    )]
    pub email: Option<String>,

    /// Override gRPC server port (default from config.yaml)
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

    /// List available endpoints for the given email
    #[arg(long, help = "List all available endpoints for the specified email")]
    pub list_endpoints: bool,
}

// Add this function to handle endpoint listing
pub async fn list_endpoints_for_email(
    email: &str,
    api_url: Option<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    use crate::endpoint_client::{check_endpoint_service_health, get_default_endpoints};

    info!("Listing endpoints for email: {}", email);

    // Validate email
    if let Err(e) = validate_email(email) {
        error!("Invalid email: {}", e);
        return Err(format!("Invalid email format: {}", e).into());
    }

    // Determine API URL
    let final_api_url = match api_url {
        Some(url) => url,
        None => match get_default_api_url().await {
            Ok(url) => {
                info!("Using default API URL from config: {}", url);
                url
            }
            Err(e) => {
                return Err(format!("No API URL provided and failed to get default: {}", e).into());
            }
        },
    };

    info!("Connecting to endpoint service at: {}", final_api_url);

    // Check service health first
    match check_endpoint_service_health(&final_api_url).await {
        Ok(true) => {
            info!("✅ Endpoint service is available");
        }
        Ok(false) => {
            return Err("❌ Endpoint service is not responding".into());
        }
        Err(e) => {
            return Err(format!("❌ Failed to connect to endpoint service: {}", e).into());
        }
    }

    // Fetch endpoints
    match get_default_endpoints(&final_api_url, email).await {
        Ok(endpoints) => {
            println!("\n📋 Available Endpoints for '{}':", email);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

            if endpoints.is_empty() {
                println!("❌ No endpoints found for this email address.");
                println!("\nPossible reasons:");
                println!("  • Email is not registered in the system");
                println!("  • No endpoints configured for this user");
                println!("  • Service has no data available");
                return Ok(());
            }

            for (index, endpoint) in endpoints.iter().enumerate() {
                println!("\n🔹 Endpoint #{}", index + 1);
                println!("   ID: {}", endpoint.id);
                println!("   Text: \"{}\"", endpoint.text);
                println!("   Description: {}", endpoint.description);

                if !endpoint.parameters.is_empty() {
                    println!("   Parameters:");
                    for param in &endpoint.parameters {
                        let required_text = match param.required.as_str() {
                            "true" => "required",
                            _ => "optional",
                        };
                        println!(
                            "     • {} ({}): {}",
                            param.name, required_text, param.description
                        );

                        if !param.alternatives.is_empty() {
                            println!("       Alternatives: {}", param.alternatives.join(", "));
                        }
                    }
                } else {
                    println!("   Parameters: None");
                }
            }

            println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("📊 Total: {} endpoints found", endpoints.len());
        }
        Err(e) => {
            return Err(format!("Failed to fetch endpoints: {}", e).into());
        }
    }

    Ok(())
}

pub async fn handle_cli(
    mut cli: Cli,
    provider: Arc<dyn ModelProvider>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Handle list endpoints command
    if cli.list_endpoints {
        let email = match &cli.email {
            Some(email) => email.clone(),
            None => {
                error!("Email is required when listing endpoints");
                return Err(
                    "Email is required when listing endpoints. Please provide it with --email"
                        .into(),
                );
            }
        };

        // If API URL not provided in CLI, try to get default from config
        if cli.api.is_none() {
            match get_default_api_url().await {
                Ok(url) => {
                    info!("Using default API URL from config: {}", url);
                    cli.api = Some(url);
                }
                Err(e) => {
                    return Err(
                        format!("No API URL provided and failed to get default: {}", e).into(),
                    );
                }
            }
        }

        return list_endpoints_for_email(&email, cli.api).await;
    }

    if let Some(prompt) = cli.prompt.clone() {
        // Email is required when analyzing a sentence
        let email = match &cli.email {
            Some(email) => {
                // Validate email
                if let Err(e) = validate_email(email) {
                    error!("Invalid email: {}", e);
                    return Err(
                        format!("Email is required when analyzing a sentence: {}", e).into(),
                    );
                }
                email.clone()
            }
            None => {
                error!("Email is required when analyzing a sentence");
                return Err(
                    "Email is required when analyzing a sentence. Please provide it with --email"
                        .into(),
                );
            }
        };

        info!("Using {} API for analysis", cli.provider);

        // If API URL not provided in CLI, try to get default from config
        if cli.api.is_none() {
            match get_default_api_url().await {
                Ok(url) => {
                    info!("Using default API URL from config: {}", url);
                    cli.api = Some(url);
                }
                Err(e) => {
                    info!(
                        "Could not get default API URL, using local endpoints: {}",
                        e
                    );
                }
            }
        }

        let endpoint_source = match &cli.api {
            Some(api_url) => format!("remote API ({})", api_url),
            None => "local file".to_string(),
        };

        info!("Using endpoints from {}", endpoint_source);
        info!("Analyzing prompt via CLI: {}", prompt);

        // Pass the API URL and email to analyze_sentence
        let result = analyze_sentence_enhanced(&prompt, provider, cli.api, &email).await?;

        println!("\nAnalysis Results:");
        println!(
            "Endpoint: {} ({})",
            result.endpoint_id, result.endpoint_description
        );
        println!("\nParameters:");
        for param in result.parameters {
            println!("\n{} ({}):", param.name, param.description);
            if let Some(semantic) = param.value {
                println!("  Semantic Match: {}", semantic);
            }
        }

        println!("\nRaw JSON Output:");
        println!("{}", serde_json::to_string_pretty(&result.raw_json)?);
    }
    Ok(())
}
