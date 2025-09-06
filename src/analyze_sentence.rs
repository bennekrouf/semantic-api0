use crate::endpoint_client::{check_endpoint_service_health, get_enhanced_endpoints};
use crate::general_question_handler::handle_general_question;
use crate::models::providers::ModelProvider;
use crate::models::{EnhancedAnalysisResult, ParameterMatch};
use crate::utils::email::validate_email;
use crate::workflow::classify_intent::{classify_intent, IntentType};
use crate::workflow::find_closest_endpoint::find_closest_endpoint;
use crate::workflow::match_fields::match_fields_semantic;
use crate::workflow::sentence_to_json::sentence_to_json;
use crate::workflow::{WorkflowConfig, WorkflowContext, WorkflowEngine, WorkflowStep};

use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;
use tracing::{error, info};

// Enhanced configuration loading step that extends the existing workflow
pub struct EnhancedConfigurationLoadingStep {
    pub api_url: Option<String>,
    pub email: String,
}

#[async_trait]
impl WorkflowStep for EnhancedConfigurationLoadingStep {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("Loading enhanced configurations with complete endpoint metadata");

        if self.email.is_empty() {
            return Err("Email is required and cannot be empty".into());
        }

        validate_email(&self.email)?;
        context.email = Some(self.email.clone());

        let api_url = self.api_url.as_ref().ok_or("No API URL provided")?;

        match check_endpoint_service_health(api_url).await {
            Ok(true) => {
                info!("Remote endpoint service available, fetching enhanced endpoints");

                match get_enhanced_endpoints(api_url, &self.email).await {
                    Ok(enhanced_endpoints) => {
                        if enhanced_endpoints.is_empty() {
                            return Err(format!(
                                "No endpoints found for user '{}'. Contact administrator.",
                                self.email
                            )
                            .into());
                        }

                        // Convert enhanced endpoints to regular endpoints for workflow compatibility
                        let regular_endpoints: Vec<crate::models::Endpoint> = enhanced_endpoints
                            .iter()
                            .map(|e| crate::models::Endpoint {
                                id: e.id.clone(),
                                text: e.text.clone(),
                                description: e.description.clone(),
                                parameters: e.parameters.clone(),
                            })
                            .collect();

                        context.endpoints_config = Some(crate::models::ConfigFile {
                            endpoints: regular_endpoints,
                        });

                        // Store enhanced endpoints for later use
                        context.enhanced_endpoints = Some(enhanced_endpoints);

                        info!(
                            "Successfully loaded {} enhanced endpoints",
                            context.enhanced_endpoints.as_ref().unwrap().len()
                        );
                    }
                    Err(e) => {
                        return Err(format!("Failed to fetch enhanced endpoints: {}", e).into());
                    }
                }
            }
            Ok(false) | Err(_) => {
                return Err("Remote endpoint service is unavailable".into());
            }
        }

        // Load model configurations (existing functionality)
        let models_config = crate::models::config::load_models_config().await?;
        context.models_config = Some(models_config);

        Ok(())
    }

    fn name(&self) -> &'static str {
        "enhanced_configuration_loading"
    }
}

// Reuse existing workflow steps
pub struct JsonGenerationStep;
pub struct EndpointMatchingStep;
pub struct FieldMatchingStep;

#[async_trait]
impl WorkflowStep for JsonGenerationStep {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let json_result = sentence_to_json(&context.sentence, context.provider.clone()).await?;
        context.json_output = Some(json_result);
        Ok(())
    }
    fn name(&self) -> &'static str {
        "json_generation"
    }
}

#[async_trait]
impl WorkflowStep for EndpointMatchingStep {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = context
            .endpoints_config
            .as_ref()
            .ok_or("Endpoints config not loaded")?;
        let endpoint_result =
            find_closest_endpoint(config, &context.sentence, context.provider.clone()).await?;

        context.endpoint_id = Some(endpoint_result.id.clone());
        context.endpoint_description = Some(endpoint_result.description.clone());
        context.matched_endpoint = Some(endpoint_result);
        Ok(())
    }
    fn name(&self) -> &'static str {
        "endpoint_matching"
    }
}

#[async_trait]
impl WorkflowStep for FieldMatchingStep {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
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

        let parameters: Vec<crate::models::EndpointParameter> = endpoint
            .parameters
            .iter()
            .map(|param| {
                let semantic_value = semantic_results
                    .iter()
                    .find(|(name, _, _)| name == &param.name)
                    .and_then(|(_, _, value)| value.clone());

                crate::models::EndpointParameter {
                    name: param.name.clone(),
                    description: param.description.clone(),
                    semantic_value,
                    alternatives: param.alternatives.clone(),
                    required: param.required,
                }
            })
            .collect();

        context.parameters = parameters;
        Ok(())
    }
    fn name(&self) -> &'static str {
        "field_matching"
    }
}

// Enhanced analysis function with intent classification
pub async fn analyze_sentence_enhanced(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    api_url: Option<String>,
    email: &str,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    if email.is_empty() {
        return Err("Email is required".into());
    }
    validate_email(email)?;
    info!(
        "Starting enhanced sentence analysis using workflow engine for: {}",
        sentence
    );

    // First, get available endpoints to classify intent
    let api_url_ref = api_url.as_ref().ok_or("No API URL provided")?;
    let enhanced_endpoints = get_enhanced_endpoints(api_url_ref, email).await?;
    let endpoint_descriptions: Vec<String> = enhanced_endpoints
        .iter()
        .map(|e| e.description.clone())
        .collect();

    // Classify intent first
    let intent = classify_intent(sentence, &endpoint_descriptions, provider.clone()).await?;

    match intent {
        IntentType::ActionableRequest => {
            info!("Processing as actionable request - running full workflow");

            // Run the full workflow for actionable requests
            const ENHANCED_WORKFLOW_CONFIG: &str = r#"
steps:
  - name: enhanced_configuration_loading
    enabled: true
    retry:
      max_attempts: 3
      delay_ms: 1000
  - name: json_generation
    enabled: true
    retry:
      max_attempts: 3
      delay_ms: 1000
  - name: endpoint_matching
    enabled: true
    retry:
      max_attempts: 2
      delay_ms: 500
  - name: field_matching
    enabled: true
    retry:
      max_attempts: 2
      delay_ms: 500
"#;

            let config: WorkflowConfig = serde_yaml::from_str(ENHANCED_WORKFLOW_CONFIG)?;
            let mut engine = WorkflowEngine::new();

            // Register all workflow steps
            for step_config in config.steps {
                match step_config.name.as_str() {
                    "enhanced_configuration_loading" => {
                        engine.register_step(
                            step_config,
                            Arc::new(EnhancedConfigurationLoadingStep {
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

            // Execute the workflow
            let context = engine.execute(sentence.to_string(), provider).await?;

            // Extract enhanced endpoint data from context
            let enhanced_endpoint = context
                .enhanced_endpoints
                .as_ref()
                .and_then(|endpoints| {
                    endpoints
                        .iter()
                        .find(|e| context.endpoint_id.as_ref().map_or(false, |id| e.id == *id))
                })
                .ok_or("Enhanced endpoint data not found")?;

            // Build parameter matches from workflow results
            let parameter_matches: Vec<ParameterMatch> = context
                .parameters
                .into_iter()
                .map(|param| ParameterMatch {
                    name: param.name,
                    description: param.description,
                    value: param.semantic_value,
                })
                .collect();

            // Return enhanced result with complete endpoint metadata
            Ok(EnhancedAnalysisResult {
                endpoint_id: enhanced_endpoint.id.clone(),
                endpoint_name: enhanced_endpoint.name.clone(),
                endpoint_description: enhanced_endpoint.description.clone(),
                verb: enhanced_endpoint.verb.clone(),
                base: enhanced_endpoint.base.clone(),
                path: enhanced_endpoint.path.clone(),
                essential_path: enhanced_endpoint.essential_path.clone(),
                api_group_id: enhanced_endpoint.api_group_id.clone(),
                api_group_name: enhanced_endpoint.api_group_name.clone(),
                parameters: parameter_matches,
                raw_json: context.json_output.ok_or("JSON output not available")?,
            })
        }

        IntentType::GeneralQuestion => {
            info!("Processing as general question - generating conversational response");

            // Handle general questions with a simple response
            let conversational_response = handle_general_question(sentence, provider).await?;

            // Return a mock EnhancedAnalysisResult for general conversations
            Ok(EnhancedAnalysisResult {
                endpoint_id: "general_conversation".to_string(),
                endpoint_name: "General Conversation".to_string(),
                endpoint_description: "Conversational response to general question".to_string(),
                verb: "GET".to_string(),
                base: "conversation".to_string(),
                path: "/general".to_string(),
                essential_path: "/general".to_string(),
                api_group_id: "conversation".to_string(),
                api_group_name: "Conversation API".to_string(),
                parameters: vec![], // No parameters for general questions
                raw_json: serde_json::json!({
                    "type": "general_conversation",
                    "response": conversational_response,
                    "intent": "general_question"
                }),
            })
        }
    }
}
