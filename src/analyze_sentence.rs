use crate::endpoint_client::endpoint;
use crate::endpoint_client::{check_endpoint_service_health, convert_remote_endpoints};
// use crate::models::config::is_debug_mode_with_local_endpoints;
use crate::endpoint_client::get_default_endpoints;
use crate::models::config::load_models_config;
use crate::models::providers::ModelProvider;
use crate::models::ConfigFile;
use crate::models::EndpointParameter;
use crate::utils::email::validate_email;
use crate::workflow::find_closest_endpoint::find_closest_endpoint;
use crate::workflow::match_fields::match_fields_semantic;
use crate::workflow::sentence_to_json::sentence_to_json;
use crate::workflow::WorkflowEngine;
use crate::workflow::WorkflowStep;
use crate::workflow::{WorkflowConfig, WorkflowContext};
use serde_json::Value;
use std::error::Error;
use tracing::{debug, error, info};

pub struct AnalysisResult {
    pub json_output: Value,
    pub endpoint_id: String,
    pub endpoint_description: String,
    pub parameters: Vec<EndpointParameter>,
}

use async_trait::async_trait;
use std::sync::Arc;

// Step 2: Define each workflow step
pub struct ConfigurationLoadingStep {
    pub api_url: Option<String>,
    pub email: String,
}

// In src/analyze_sentence.rs - Update the ConfigurationLoadingStep

#[async_trait]
impl WorkflowStep for ConfigurationLoadingStep {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("Loading configurations from remote endpoint service");

        // Validate email - fail early if invalid
        if self.email.is_empty() {
            return Err("Email is required and cannot be empty".into());
        }

        if let Err(e) = validate_email(&self.email) {
            error!("ERROR: {}", e);
            error!("Please provide a valid email address in the format user@example.com");
            error!("\nExample:");
            error!("  cargo run -- --provider ollama --email user@example.com");
            std::process::exit(1);
        }

        // Set email in context
        context.email = Some(self.email.clone());
        // Ensure API URL is provided
        let api_url = self.api_url.as_ref().ok_or("No API URL provided")?;

        // First verify the service is available
        match check_endpoint_service_health(api_url).await {
            Ok(true) => {
                info!("Remote endpoint service is available, fetching endpoints");

                // Use the new get_default_endpoints function
                match get_default_endpoints(api_url, &self.email).await {
                    Ok(remote_endpoints) => {
                        // Check if we got any endpoints - FAIL EARLY if none
                        if remote_endpoints.is_empty() {
                            error!("No endpoints available for email: {}", self.email);
                            return Err(format!(
                                "No endpoints found for user '{}'. Please check your email address or contact your administrator to configure endpoints for your account.",
                                self.email
                            ).into());
                        }

                        // Convert and store endpoints
                        let endpoints = convert_remote_endpoints(
                            // We'll need to wrap endpoints in an ApiGroup to use the converter
                            vec![endpoint::ApiGroup {
                                id: "default".to_string(),
                                name: "Default Group".to_string(),
                                description: "Default API Group".to_string(),
                                base: "".to_string(),
                                endpoints: remote_endpoints,
                            }],
                        );

                        let endpoints_len = endpoints.len();

                        let config = ConfigFile { endpoints };
                        context.endpoints_config = Some(config);

                        info!("Successfully loaded {} endpoints", endpoints_len);
                    }
                    Err(e) => {
                        error!("Failed to fetch endpoints: {}", e);
                        return Err(format!(
                            "Failed to fetch endpoints from remote service: {}",
                            e
                        )
                        .into());
                    }
                }
            }
            Ok(false) | Err(_) => {
                return Err("Remote endpoint service is unavailable. Please check the service status or contact your administrator.".into());
            }
        }

        // Load model configurations
        let models_config = load_models_config().await?;
        context.models_config = Some(models_config);

        Ok(())
    }

    fn name(&self) -> &'static str {
        "configuration_loading"
    }
}

// Step 2.2: JSON Generation Step
pub struct JsonGenerationStep;

#[async_trait]
impl WorkflowStep for JsonGenerationStep {
    async fn execute(
        &self,
        context: &mut crate::workflow::context::WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("Generating JSON from sentence");

        let json_result = sentence_to_json(&context.sentence, context.provider.clone()).await?;
        context.json_output = Some(json_result);

        debug!("JSON generation successful");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "json_generation"
    }
}

// Step 2.3: Endpoint Matching Step
pub struct EndpointMatchingStep;

#[async_trait]
impl WorkflowStep for EndpointMatchingStep {
    async fn execute(
        &self,
        context: &mut crate::workflow::context::WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("Finding closest matching endpoint");

        let config = context
            .endpoints_config
            .as_ref()
            .ok_or("Endpoints configuration not loaded")?;

        let endpoint_result =
            find_closest_endpoint(config, &context.sentence, context.provider.clone()).await?;
        context.endpoint_id = Some(endpoint_result.id.clone());
        context.endpoint_description = Some(endpoint_result.description.clone());
        context.matched_endpoint = Some(endpoint_result);

        debug!("Endpoint matching successful");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "endpoint_matching"
    }
}

// Step 2.4: Field Matching Step
pub struct FieldMatchingStep;

#[async_trait]
impl WorkflowStep for FieldMatchingStep {
    async fn execute(
        &self,
        context: &mut crate::workflow::context::WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("Performing field matching");

        let json_output = context
            .json_output
            .as_ref()
            .ok_or("JSON output not available")?;
        let endpoint = context
            .matched_endpoint
            .as_ref()
            .ok_or("Matched endpoint not available")?;

        let semantic_results =
            match_fields_semantic(json_output, endpoint, context.provider.clone()).await?;

        // Convert semantic results to parameters
        let parameters: Vec<EndpointParameter> = endpoint
            .parameters
            .iter()
            .map(|param| {
                let semantic_value = semantic_results
                    .iter()
                    .find(|(name, _, _)| name == &param.name)
                    .and_then(|(_, _, value)| value.clone());

                EndpointParameter {
                    name: param.name.clone(),
                    description: param.description.clone(),
                    semantic_value,
                    alternatives: param.alternatives.clone(),
                    required: param.required,
                }
            })
            .collect();

        context.parameters = parameters;

        debug!("Field matching completed");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "field_matching"
    }
}

// Step 3: Workflow Configuration (unchanged)
const WORKFLOW_CONFIG: &str = r#"
steps:
  - name: configuration_loading
    enabled: true
    retry:
      max_attempts: 3
      delay_ms: 1000
    timeout_secs: 10
  - name: json_generation
    enabled: true
    retry:
      max_attempts: 3
      delay_ms: 1000
    timeout_secs: 30
  - name: endpoint_matching
    enabled: true
    retry:
      max_attempts: 2
      delay_ms: 500
    timeout_secs: 20
  - name: field_matching
    enabled: true
    retry:
      max_attempts: 2
      delay_ms: 500
    timeout_secs: 20
"#;

// Step 4: Updated analyze_sentence function with API URL parameter
pub async fn analyze_sentence(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    api_url: Option<String>,
    email: &str,
) -> Result<AnalysisResult, Box<dyn Error + Send + Sync>> {
    // Validate email before proceeding
    if email.is_empty() {
        return Err("Email is required and cannot be empty".into());
    }

    // Validate email format
    let email_regex = regex::Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
        .expect("Invalid email regex pattern");

    if !email_regex.is_match(email) {
        return Err(format!("Invalid email format: {}", email).into());
    }

    info!("Starting sentence analysis for: {}", sentence);
    info!("Using email: {}", email);

    if let Some(url) = &api_url {
        info!("Using remote API for endpoints: {}", url);
    } else {
        info!("Using local endpoints file");
    }

    // Initialize workflow engine
    let config: WorkflowConfig = serde_yaml::from_str(WORKFLOW_CONFIG)?;
    let mut engine = WorkflowEngine::new();

    // Register all steps
    for step_config in config.steps {
        match step_config.name.as_str() {
            "configuration_loading" => {
                engine.register_step(
                    step_config,
                    Arc::new(ConfigurationLoadingStep {
                        api_url: api_url.clone(),
                        email: email.to_string(),
                    }),
                );
            }
            "json_generation" => {
                engine.register_step(step_config, Arc::new(JsonGenerationStep));
            }
            "endpoint_matching" => {
                engine.register_step(step_config, Arc::new(EndpointMatchingStep));
            }
            "field_matching" => {
                engine.register_step(step_config, Arc::new(FieldMatchingStep));
            }
            _ => {
                error!("Unknown step: {}", step_config.name);
                return Err(format!("Unknown step: {}", step_config.name).into());
            }
        }
    }

    // Execute workflow
    let context = engine.execute(sentence.to_string(), provider).await?;

    // Convert workflow context to analysis result
    Ok(AnalysisResult {
        json_output: context.json_output.ok_or("JSON output not available")?,
        endpoint_id: context.endpoint_id.ok_or("Endpoint ID not available")?,
        endpoint_description: context
            .endpoint_description
            .ok_or("Endpoint description not available")?,
        parameters: context.parameters,
    })
}
