// src/cli.rs - Updated to use only Cohere
use clap::Parser;
use std::{error::Error, sync::Arc};
use crate::app_log;

use crate::comparison_test::run_model_comparison;
use crate::endpoint_client::get_default_api_url;
use crate::utils::email::validate_email;
use crate::workflow::classify_intent::IntentType;
use crate::{analyze_sentence::analyze_sentence_enhanced, models::providers::ModelProvider};

pub fn display_custom_help() {
    println!(
        "
╭─────────────────────────────────────────────────╮
│                  Semantic                       │
│         Natural Language API Matcher            │
│           with Intent Classification            │
╰─────────────────────────────────────────────────╯

ARGUMENTS:
  --provider PROVIDER  AI provider to use
                       Options: cohere, claude, deepseek
                       Default: cohere
  --email ADDRESS    Your email address 
                     (REQUIRED ONLY when analyzing a sentence)

  --api URL          Remote API endpoint for fetching endpoints
                     Default: Uses local endpoints.yaml
  
  --port PORT        Override gRPC server port
                     Default: From config.yaml

USAGE EXAMPLES:
  1. Start gRPC server (no email required):
     semantic
     semantic --provider deepseek
  
  2. Analyze text (email required):
     semantic --email user@example.com \"analyze this text\"
     semantic --provider deepseek --email user@example.com \"help me\"
  
  3. Use remote endpoints:
     semantic --api http://example.com:50053 --email user@example.com \"what can i do\"

  4. Run standard comparison test:
     semantic --compare --iterations 10

  5. Run enhanced intent classification test:
     semantic --compare-intents --iterations 5

  6. List available endpoints:
     semantic --list-endpoints --email user@example.com

INTENT TYPES SUPPORTED:
  📋 Actionable Request: \"Send email to john@example.com\"
  💬 General Question: \"What is machine learning?\"
  ❓ Help Request: \"What can I do?\" / \"Que puis-je faire?\"

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

    #[arg(long, help = "Run comparison test between models and prompt versions")]
    pub compare: bool,

    #[arg(long, help = "Run enhanced intent classification comparison test")]
    pub compare_intents: bool,

    #[arg(
        long,
        default_value = "20",
        help = "Number of iterations per test configuration"
    )]
    pub iterations: u32,
}

// Update handle_cli function to handle enhanced intent testing:
pub async fn handle_cli(
    mut cli: Cli,
    provider: Arc<dyn ModelProvider>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if cli.compare {
        let config = crate::comparison_test::TestConfig {
            iterations: cli.iterations,
            ..Default::default() // Use all defaults from TestConfig
        };
        crate::comparison_test::run_custom_comparison(config).await?;
        return Ok(());
    }

    if cli.compare_intents {
        run_model_comparison().await?;

        return Ok(());
    }

    // Handle list endpoints command
    if cli.list_endpoints {
        let email = match &cli.email {
            Some(email) => email.clone(),
            None => {
                app_log!(error, "Email is required when listing endpoints");
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
                    app_log!(info, "Using default API URL from config: {}", url);
                    cli.api = Some(url);
                }
                Err(e) => {
                    return Err(
                        format!("No API URL provided and failed to get default: {e}").into(),
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
                    app_log!(error, "Invalid email: {}", e);
                    return Err(format!("Email is required when analyzing a sentence: {e}").into());
                }
                email.clone()
            }
            None => {
                app_log!(error, "Email is required when analyzing a sentence");
                return Err(
                    "Email is required when analyzing a sentence. Please provide it with --email"
                        .into(),
                );
            }
        };

        app_log!(info, "Using {} API for analysis", cli.provider);

        // If API URL not provided in CLI, try to get default from config
        if cli.api.is_none() {
            match get_default_api_url().await {
                Ok(url) => {
                    app_log!(info, "Using default API URL from config: {}", url);
                    cli.api = Some(url);
                }
                Err(e) => {
                    app_log!(info, 
                        "Could not get default API URL, using local endpoints: {}",
                        e
                    );
                }
            }
        }

        let endpoint_source = match &cli.api {
            Some(api_url) => format!("remote API ({api_url})"),
            None => "local file".to_string(),
        };

        app_log!(info, "Using endpoints from {}", endpoint_source);
        app_log!(info, "Analyzing prompt via CLI: {}", prompt);

        // Pass the API URL and email to analyze_sentence
        let result = analyze_sentence_enhanced(&prompt, provider, cli.api, &email, None).await?;

        println!("\nAnalysis Results:");
        println!(
            "Intent: {:?}",
            match result.intent {
                IntentType::ActionableRequest => "Actionable Request",
                IntentType::GeneralQuestion => "General Question",
                IntentType::HelpRequest => "Help Request",
            }
        );
        println!(
            "Endpoint: {} ({})",
            result.endpoint_id, result.endpoint_description
        );

        println!("\nUsage Information:");
        println!("  Model: {}", result.usage.model);
        println!("  Input tokens: {}", result.usage.input_tokens);
        println!("  Output tokens: {}", result.usage.output_tokens);
        println!("  Total tokens: {}", result.usage.total_tokens);
        println!(
            "  Estimated: {}",
            if result.usage.estimated { "Yes" } else { "No" }
        );

        // Show response content for help/general questions
        match result.intent {
            IntentType::HelpRequest | IntentType::GeneralQuestion => {
                if let Some(response) = result.raw_json.get("response").and_then(|v| v.as_str()) {
                    println!("\nResponse:");
                    println!("{response}");
                }
            }
            IntentType::ActionableRequest => {
                println!("\nParameters:");
                for param in result.parameters {
                    println!("\n{} ({}):", param.name, param.description);
                    if let Some(semantic) = param.value {
                        println!("  Semantic Match: {semantic}");
                    }
                }

                println!("\nRaw JSON Output:");
                println!("{}", serde_json::to_string_pretty(&result.raw_json)?);

                let status_text = match result.matching_info.status {
                    crate::models::MatchingStatus::Complete => "Complete",
                    crate::models::MatchingStatus::Partial => "Partial",
                    crate::models::MatchingStatus::Incomplete => "Incomplete",
                };

                println!(
                    "Matching Status: {} ({:.1}% complete)",
                    status_text, result.matching_info.completion_percentage
                );
            }
        }
    }
    Ok(())
}

// Add this function to handle endpoint listing
pub async fn list_endpoints_for_email(
    email: &str,
    api_url: Option<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    use crate::endpoint_client::{check_endpoint_service_health, get_default_endpoints};

    app_log!(info, "Listing endpoints for email: {}", email);

    // Validate email
    if let Err(e) = validate_email(email) {
        app_log!(error, "Invalid email: {}", e);
        return Err(format!("Invalid email format: {e}").into());
    }

    // Determine API URL
    let final_api_url = match api_url {
        Some(url) => url,
        None => match get_default_api_url().await {
            Ok(url) => {
                app_log!(info, "Using default API URL from config: {}", url);
                url
            }
            Err(e) => {
                return Err(format!("No API URL provided and failed to get default: {e}").into());
            }
        },
    };

    app_log!(info, "Connecting to endpoint service at: {}", final_api_url);

    // Check service health first
    match check_endpoint_service_health(&final_api_url).await {
        Ok(true) => {
            app_log!(info, "✅ Endpoint service is available");
        }
        Ok(false) => {
            return Err("❌ Endpoint service is not responding".into());
        }
        Err(e) => {
            return Err(format!("❌ Failed to connect to endpoint service: {e}").into());
        }
    }

    // Fetch endpoints
    match get_default_endpoints(&final_api_url, email).await {
        Ok(endpoints) => {
            println!("\n📋 Available Endpoints for '{email}':");
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
            return Err(format!("Failed to fetch endpoints: {e}").into());
        }
    }

    Ok(())
}
