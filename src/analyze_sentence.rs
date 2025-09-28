use crate::endpoint_client::{check_endpoint_service_health, get_enhanced_endpoints};
use crate::general_question_handler::handle_general_question;
use crate::models::providers::ModelProvider;
use crate::models::{
    EnhancedAnalysisResult, MatchingInfo, MatchingStatus, MissingField, ParameterMatch, UsageInfo
};
use crate::utils::email::validate_email;
use crate::workflow::classify_intent::IntentType;
use crate::workflow::find_closest_endpoint::find_closest_endpoint;
use crate::workflow::match_fields::match_fields_semantic;
use crate::workflow::sentence_to_json::sentence_to_json;
use crate::workflow::{WorkflowConfig, WorkflowContext, WorkflowEngine, WorkflowStep};
use crate::workflow::actions::classify_intent::classify_intent;
use crate::help_response_handler::handle_help_request;

use crate::progressive_matching::ProgressiveMatchingManager;

use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

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

        // The sentence_to_json function should return usage info, but since it doesn't,
        // we need to estimate the tokens used in this step
        let enhanced_calculator = crate::utils::token_calculator::EnhancedTokenCalculator::new();
        let step_usage = enhanced_calculator.calculate_usage(
            &context.sentence,
            "",
            context.provider.get_model_name(),
        );

        // Add tokens to context
        context.total_input_tokens += step_usage.input_tokens;
        context.total_output_tokens += step_usage.output_tokens;

        debug!(
            "JSON generation step added {} input tokens, {} output tokens",
            step_usage.input_tokens, step_usage.output_tokens
        );

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

        // Estimate tokens for endpoint matching step
        let enhanced_calculator = crate::utils::token_calculator::EnhancedTokenCalculator::new();
        let step_usage = enhanced_calculator.calculate_usage(
            &context.sentence,
            "",
            context.provider.get_model_name(),
        );

        // Add tokens to context
        context.total_input_tokens += step_usage.input_tokens;
        context.total_output_tokens += step_usage.output_tokens;

        debug!(
            "Endpoint matching step added {} input tokens, {} output tokens",
            step_usage.input_tokens, step_usage.output_tokens
        );

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

        // Estimate tokens for field matching step
        let enhanced_calculator = crate::utils::token_calculator::EnhancedTokenCalculator::new();
        let step_usage = enhanced_calculator.calculate_usage(
            &context.sentence,
            "",
            context.provider.get_model_name(),
        );

        // Add tokens to context
        context.total_input_tokens += step_usage.input_tokens;
        context.total_output_tokens += step_usage.output_tokens;

        debug!(
            "Field matching step added {} input tokens, {} output tokens",
            step_usage.input_tokens, step_usage.output_tokens
        );

        Ok(())
    }
    fn name(&self) -> &'static str {
        "field_matching"
    }
}

// Retry logic for actionable analysis
async fn analyze_with_retry(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    api_url: Option<String>,
    email: &str,
    conversation_id: Option<String>,
    retry_attempts: u32,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let mut last_error = None;

    for attempt in 1..=retry_attempts {
        info!(
            "Analysis attempt {}/{} for: {}",
            attempt, retry_attempts, sentence
        );

        match try_actionable_analysis(
            sentence,
            provider.clone(),
            api_url.clone(),
            email,
            conversation_id.clone(),
        )
        .await
        {
            Ok(result) => {
                info!("Analysis succeeded on attempt {}", attempt);
                return Ok(result);
            }
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("No suitable endpoint found")
                    || error_msg.contains("not found in available endpoints")
                {
                    warn!(
                        "Endpoint matching failed on attempt {}: {}",
                        attempt, error_msg
                    );
                    last_error = Some(e);

                    if attempt < retry_attempts {
                        // Add small delay between retries
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        continue;
                    }
                } else {
                    // For other errors, don't retry
                    return Err(e);
                }
            }
        }
    }

    // If we get here, all retries failed
    Err(last_error.unwrap_or_else(|| "Analysis failed after retries".into()))
}

// Extract the actionable analysis logic into this function
async fn try_actionable_analysis(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    api_url: Option<String>,
    email: &str,
    conversation_id: Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    // let model = provider.get_model_name().to_string();

    // Run the full workflow for actionable requests
    const ENHANCED_WORKFLOW_CONFIG: &str = r#"
steps:
  - name: enhanced_configuration_loading
    enabled: true
    retry:
      max_attempts: 3
      delay_ms: 1000
  - name: endpoint_matching  # Do endpoint matching FIRST
    enabled: true
    retry:
      max_attempts: 2
      delay_ms: 500
  - name: json_generation    # Then extract parameters for the specific endpoint
    enabled: true
    retry:
      max_attempts: 3
      delay_ms: 1000
  - name: field_matching     # Finally do field matching as cleanup
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
                engine.register_step(
                    step_config,
                    Arc::new(EndpointMatchingStep), // Uses the updated implementation
                );
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
    let context = engine
        .execute(sentence.to_string(), provider.clone())
        .await?;

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

    let matching_info = MatchingInfo::compute(&parameter_matches, &enhanced_endpoint.parameters);
    let user_prompt = matching_info.generate_user_prompt(&enhanced_endpoint.name);

    // If workflow didn't track tokens properly, estimate them based on the sentence and response
     let (final_input_tokens, final_output_tokens) = if context.total_output_tokens == 0 {
        debug!("Workflow reported 0 output tokens, estimating output tokens");

        let enhanced_calculator = crate::utils::token_calculator::EnhancedTokenCalculator::new();

        // Use existing input tokens if available, otherwise estimate
        let estimated_input = if context.total_input_tokens > 0 {
            context.total_input_tokens
        } else {
            let sentence_tokens = enhanced_calculator.estimate_tokens_enhanced(sentence, provider.get_model_name(), None);
            sentence_tokens * 3 // Used in ~3 different LLM calls
        };

        // Estimate output tokens based on all the content generated by the workflow
        let mut total_output_content = String::new();

        // Add JSON output content
        if let Some(json_output) = &context.json_output {
            let json_str = serde_json::to_string(json_output).unwrap_or_default();
            total_output_content.push_str(&json_str);
            total_output_content.push_str(" ");
        }

        // Add endpoint matching result (endpoint ID and description)
        if let Some(endpoint_id) = &context.endpoint_id {
            total_output_content.push_str(endpoint_id);
            total_output_content.push_str(" ");
        }
        if let Some(desc) = &context.endpoint_description {
            total_output_content.push_str(desc);
            total_output_content.push_str(" ");
        }
        
        // Add parameter processing results
        for param in &parameter_matches {
            total_output_content.push_str(&param.name);
            total_output_content.push_str(" ");
            if let Some(value) = &param.value {
                total_output_content.push_str(value);
                total_output_content.push_str(" ");
            }
        }
        
        // Add estimated tokens for LLM reasoning/processing overhead
        let sentence_tokens = enhanced_calculator.estimate_tokens_enhanced(sentence, provider.get_model_name(), None);
        let reasoning_overhead = sentence_tokens * 2; // Assume 2x input tokens for reasoning
        
        let content_tokens = enhanced_calculator.estimate_tokens_enhanced(&total_output_content, provider.get_model_name(), None);
        let estimated_output = content_tokens + reasoning_overhead;
        
        debug!("Output estimation breakdown: content='{}' ({} tokens), reasoning overhead ({} tokens), total output: {}", 
               total_output_content.chars().take(100).collect::<String>(), 
               content_tokens, reasoning_overhead, estimated_output);
               
        (estimated_input, estimated_output)
    } else {
        (context.total_input_tokens, context.total_output_tokens)
    };

    // Create usage info from final token counts
    let usage_info = UsageInfo {
        input_tokens: final_input_tokens,
        output_tokens: final_output_tokens,
        total_tokens: final_input_tokens + final_output_tokens,
        model: provider.get_model_name().to_string(),
        estimated: true, // Workflow aggregates multiple calls, so mark as estimated
    };

    debug!(
        "Final workflow token usage: input={}, output={}, total={}",
        usage_info.input_tokens, usage_info.output_tokens, usage_info.total_tokens
    );

    // Return enhanced result with complete endpoint metadata
    Ok(EnhancedAnalysisResult {
        conversation_id,
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
        matching_info,
        user_prompt,
        total_input_tokens: final_input_tokens,
        total_output_tokens: final_output_tokens,
        usage: usage_info,
        intent: IntentType::ActionableRequest,
    })
}

// Enhanced analysis function with progressive matching as FIRST priority
pub async fn analyze_sentence_enhanced(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    api_url: Option<String>,
    email: &str,
    conversation_id: Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let model = provider.get_model_name().to_string();
    if email.is_empty() {
        return Err("Email is required".into());
    }
    validate_email(email)?;

    let analysis_config = crate::models::config::load_analysis_config()
        .await
        .unwrap_or_default();

    info!(
        "Starting enhanced sentence analysis with {} retry attempts for: {}",
        analysis_config.retry_attempts, sentence
    );

    let api_url_ref = api_url.as_ref().ok_or("No API URL provided")?;

    // STEP 1: PROGRESSIVE MATCHING CHECK (HIGHEST PRIORITY)
    // If we have a conversation_id, check for ongoing requests FIRST
    if let Some(ref conv_id) = conversation_id {
        info!("Checking for ongoing progressive match for conversation: {}", conv_id);
        
        if let Ok(db_url) = crate::progressive_matching::get_database_url() {
            if let Ok(progressive_manager) = ProgressiveMatchingManager::new(&db_url).await {
                // Check if there's an ongoing incomplete match
                match progressive_manager.get_incomplete_match(conv_id).await {
                    Ok(Some(ongoing_match)) => {
                        info!("Found ongoing progressive match for endpoint: {}", ongoing_match.endpoint_id);
                        
                        // Process this as a progressive follow-up
                        match handle_progressive_followup(
                            sentence,
                            conv_id,
                            &ongoing_match,
                            provider.clone(),
                            &progressive_manager,
                            api_url_ref,
                            email,
                        ).await {
                            Ok(progressive_result) => {
                                info!("Progressive matching completed successfully");
                                return Ok(progressive_result);
                            },
                            Err(e) => {
                                warn!("Progressive matching failed: {}, continuing with normal flow", e);
                                // Continue to normal flow if progressive matching fails
                            }
                        }
                    },
                    Ok(None) => {
                        debug!("No ongoing progressive match found for conversation: {}", conv_id);
                    },
                    Err(e) => {
                        warn!("Error checking for progressive match: {}, continuing with normal flow", e);
                    }
                }
            }
        }
    }

    // STEP 2: NORMAL FLOW (Intent Classification + Endpoint Matching)
    // Only reached if no progressive match was found or it failed
    info!("No progressive match found, proceeding with normal analysis flow");

    let enhanced_endpoints = get_enhanced_endpoints(api_url_ref, email).await?;
    let endpoint_descriptions: Vec<String> = enhanced_endpoints
        .iter()
        .map(|e| e.description.clone())
        .collect();

    let intent = classify_intent(sentence, &endpoint_descriptions, provider.clone()).await?;

    match intent {
        IntentType::ActionableRequest => {
            info!("Processing as NEW actionable request");
            match analyze_with_retry(
                sentence,
                provider.clone(),
                api_url,
                email,
                conversation_id.clone(),
                analysis_config.retry_attempts,
            )
            .await
            {
                Ok(result) => Ok(result),
                Err(e) => {
                    if analysis_config.fallback_to_general {
                        warn!("All retries failed, falling back to general question handler: {}", e);
                        create_fallback_response(sentence, provider, model, conversation_id).await
                    } else {
                        Err(e)
                    }
                }
            }
        }

        IntentType::HelpRequest => {
            info!("Processing as help request");
            create_help_response(sentence, &enhanced_endpoints, provider, conversation_id).await
        }

        IntentType::GeneralQuestion => {
            info!("Processing as general question");
            create_general_response(sentence, provider, model, conversation_id).await
        }
    }
}

// Dedicated progressive matching handler
async fn handle_progressive_followup(
    sentence: &str,
    conversation_id: &str,
    ongoing_match: &crate::progressive_matching::OngoingMatch,
    provider: Arc<dyn ModelProvider>,
    progressive_manager: &ProgressiveMatchingManager,
    api_url: &str,
    email: &str,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    info!("Processing progressive follow-up for endpoint: {}", ongoing_match.endpoint_id);

    // Get the endpoint definition to understand its parameters
    let enhanced_endpoints = get_enhanced_endpoints(api_url, email).await?;
    let endpoint = enhanced_endpoints
        .iter()
        .find(|e| e.id == ongoing_match.endpoint_id)
        .ok_or_else(|| format!("Endpoint {} not found", ongoing_match.endpoint_id))?;

    info!("Found endpoint: {} with {} parameters", endpoint.name, endpoint.parameters.len());

    // Extract new parameters from the follow-up message
    let new_parameters = extract_parameters_from_followup(
        sentence,
        provider.clone(),
        &endpoint.parameters,
    ).await?;

    info!("Extracted {} new parameters from follow-up", new_parameters.len());

    if new_parameters.is_empty() {
        return Err("No parameters could be extracted from the follow-up message".into());
    }

    // Update the progressive match with new parameters
    progressive_manager
        .update_match(conversation_id, &ongoing_match.endpoint_id, new_parameters.clone())
        .await?;

    // Check if we're now complete
    let required_param_names: Vec<String> = endpoint
        .parameters
        .iter()
        .filter(|p| p.required.unwrap_or(false))
        .map(|p| p.name.clone())
        .collect();

    let completion_result = progressive_manager
        .check_completion(
            conversation_id,
            &ongoing_match.endpoint_id,
            required_param_names,
            &endpoint.parameters,
        )
        .await?;

    info!(
        "Progressive matching completion: {}% complete, is_complete: {}",
        completion_result.completion_percentage, completion_result.is_complete
    );

    if completion_result.is_complete {
        // Clean up the progressive match
        progressive_manager
            .complete_match(conversation_id, &ongoing_match.endpoint_id)
            .await?;

        info!("Progressive matching completed successfully");
        create_complete_progressive_response(endpoint, completion_result, &Some(conversation_id.to_string())).await
    } else {
        info!("Progressive matching still incomplete, prompting for more parameters");
        create_partial_progressive_response(endpoint, completion_result, &Some(conversation_id.to_string())).await
    }
}

// Extract parameters from follow-up using the existing function from sentence_analysis.rs
async fn extract_parameters_from_followup(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    endpoint_parameters: &[crate::models::EndpointParameter],
) -> Result<Vec<crate::progressive_matching::ParameterValue>, Box<dyn std::error::Error + Send + Sync>> {
    info!("Extracting parameters from follow-up: '{}'", sentence);

    let prompt_manager = crate::prompts::PromptManager::new().await?;
    let available_params: Vec<String> = endpoint_parameters
        .iter()
        .map(|p| format!("{}: {}", p.name, p.description))
        .collect();
    let available_params_str = available_params.join("\n");

    let prompt = prompt_manager.format_extract_followup_parameters_with_mapping(
        sentence,
        &available_params_str,
        Some("v1"),
    )?;

    let models_config = crate::models::config::load_models_config().await?;
    let model_config = &models_config.default;

    let result = provider.generate(&prompt, model_config).await?;
    let json_result = crate::json_helper::sanitize_json(&result.content)?;

    let mut parameters = Vec::new();
    let valid_param_names: Vec<&str> = endpoint_parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();

    if let Some(obj) = json_result.as_object() {
        for (key, value) in obj {
            if let Some(str_value) = value.as_str() {
                if !str_value.trim().is_empty() && valid_param_names.contains(&key.as_str()) {
                    parameters.push(crate::progressive_matching::ParameterValue {
                        name: key.clone(),
                        value: str_value.trim().to_string(),
                        description: format!("User provided value for {key}"),
                    });
                }
            }
        }
    }

    Ok(parameters)
}

// Helper functions for creating responses
async fn create_complete_progressive_response(
    endpoint: &crate::models::EnhancedEndpoint,
    result: crate::progressive_matching::ProgressiveMatchResult,
    conversation_id: &Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let parameters: Vec<ParameterMatch> = result
        .matched_parameters
        .into_iter()
        .map(|param| ParameterMatch {
            name: param.name,
            description: param.description,
            value: Some(param.value),
        })
        .collect();

    let matching_info = MatchingInfo {
        status: MatchingStatus::Complete,
        total_required_fields: parameters.len(),
        mapped_required_fields: parameters.len(),
        total_optional_fields: 0,
        mapped_optional_fields: 0,
        completion_percentage: 100.0,
        missing_required_fields: vec![],
        missing_optional_fields: vec![],
    };

    let usage_info = UsageInfo {
        input_tokens: 50,
        output_tokens: 20,
        total_tokens: 70,
        model: "progressive_matching".to_string(),
        estimated: true,
    };

    Ok(EnhancedAnalysisResult {
        endpoint_id: endpoint.id.clone(),
        endpoint_name: endpoint.name.clone(),
        endpoint_description: endpoint.description.clone(),
        verb: endpoint.verb.clone(),
        base: endpoint.base.clone(),
        path: endpoint.path.clone(),
        essential_path: endpoint.essential_path.clone(),
        api_group_id: endpoint.api_group_id.clone(),
        api_group_name: endpoint.api_group_name.clone(),
        parameters,
        raw_json: serde_json::json!({
            "type": "progressive_complete",
            "endpoint_id": endpoint.id,
            "status": "complete",
            "completion_percentage": 100.0
        }),
        conversation_id: conversation_id.clone(),
        matching_info,
        user_prompt: None,
        total_input_tokens: usage_info.input_tokens,
        total_output_tokens: usage_info.output_tokens,
        usage: usage_info,
        intent: IntentType::ActionableRequest,
    })
}

async fn create_partial_progressive_response(
    endpoint: &crate::models::EnhancedEndpoint,
    result: crate::progressive_matching::ProgressiveMatchResult,
    conversation_id: &Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let parameters: Vec<ParameterMatch> = result
        .matched_parameters
        .into_iter()
        .map(|param| ParameterMatch {
            name: param.name,
            description: param.description,
            value: Some(param.value),
        })
        .collect();

    let missing_fields: Vec<MissingField> = result
        .missing_parameters
        .iter()
        .map(|param| MissingField {
            name: param.clone(),
            description: format!("Missing required parameter: {}", param),
        })
        .collect();

    let matching_info = MatchingInfo {
        status: MatchingStatus::Partial,
        total_required_fields: parameters.len() + missing_fields.len(),
        mapped_required_fields: parameters.len(),
        total_optional_fields: 0,
        mapped_optional_fields: 0,
        completion_percentage: result.completion_percentage,
        missing_required_fields: missing_fields,
        missing_optional_fields: vec![],
    };

    let user_prompt = generate_missing_fields_prompt(&result.missing_parameters);
    
    let usage_info = UsageInfo {
        input_tokens: 30,
        output_tokens: 15,
        total_tokens: 45,
        model: "progressive_matching".to_string(),
        estimated: true,
    };

    Ok(EnhancedAnalysisResult {
        endpoint_id: endpoint.id.clone(),
        endpoint_name: endpoint.name.clone(),
        endpoint_description: endpoint.description.clone(),
        verb: endpoint.verb.clone(),
        base: endpoint.base.clone(),
        path: endpoint.path.clone(),
        essential_path: endpoint.essential_path.clone(),
        api_group_id: endpoint.api_group_id.clone(),
        api_group_name: endpoint.api_group_name.clone(),
        parameters,
        raw_json: serde_json::json!({
            "type": "progressive_partial",
            "endpoint_id": endpoint.id,
            "status": "incomplete",
            "completion_percentage": result.completion_percentage,
            "missing_parameters": result.missing_parameters
        }),
        conversation_id: conversation_id.clone(),
        matching_info,
        user_prompt: Some(user_prompt),
        total_input_tokens: usage_info.input_tokens,
        total_output_tokens: usage_info.output_tokens,
        usage: usage_info,
        intent: IntentType::ActionableRequest,
    })
}

fn generate_missing_fields_prompt(missing_params: &[String]) -> String {
    match missing_params.len() {
        0 => "All required information has been provided.".to_string(),
        1 => format!(
            "I need one more piece of information: {}. Could you please provide it?",
            missing_params[0].replace('_', " ")
        ),
        2 => format!(
            "I need two more pieces of information: {} and {}. Could you provide them?",
            missing_params[0].replace('_', " "),
            missing_params[1].replace('_', " ")
        ),
        _ => {
            let (initial, last) = missing_params.split_at(missing_params.len() - 1);
            format!(
                "I need a few more details: {}, and {}. Could you provide this information?",
                initial
                    .iter()
                    .map(|p| p.replace('_', " "))
                    .collect::<Vec<_>>()
                    .join(", "),
                last[0].replace('_', " ")
            )
        }
    }
}

async fn create_fallback_response(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    model: String,
    conversation_id: Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let conversational_result = handle_general_question(sentence, provider).await?;

    let matching_info = MatchingInfo {
        status: MatchingStatus::Complete,
        total_required_fields: 0,
        mapped_required_fields: 0,
        total_optional_fields: 0,
        mapped_optional_fields: 0,
        completion_percentage: 100.0,
        missing_required_fields: vec![],
        missing_optional_fields: vec![],
    };

    let usage_info = UsageInfo {
        input_tokens: conversational_result.usage.input_tokens,
        output_tokens: conversational_result.usage.output_tokens,
        total_tokens: conversational_result.usage.total_tokens,
        model,
        estimated: conversational_result.usage.estimated,
    };

    Ok(EnhancedAnalysisResult {
        endpoint_id: "general_conversation_fallback".to_string(),
        endpoint_name: "General Conversation (Fallback)".to_string(),
        endpoint_description: "Fallback conversational response after endpoint matching failed".to_string(),
        verb: "GET".to_string(),
        base: "conversation".to_string(),
        path: "/general".to_string(),
        essential_path: "/general".to_string(),
        api_group_id: "conversation".to_string(),
        api_group_name: "Conversation API".to_string(),
        parameters: vec![],
        raw_json: serde_json::json!({
            "type": "general_conversation_fallback",
            "response": conversational_result.content,
            "intent": "actionable_request_failed",
            "fallback_reason": "endpoint_matching_failed_after_retries"
        }),
        conversation_id,
        matching_info,
        user_prompt: None,
        total_input_tokens: conversational_result.usage.input_tokens,
        total_output_tokens: conversational_result.usage.output_tokens,
        usage: usage_info,
        intent: IntentType::GeneralQuestion,
    })
}

async fn create_help_response(
    sentence: &str,
    enhanced_endpoints: &[crate::models::EnhancedEndpoint],
    provider: Arc<dyn ModelProvider>,
    conversation_id: Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let help_result = handle_help_request(sentence, enhanced_endpoints, provider.clone()).await?;

    let matching_info = MatchingInfo {
        status: MatchingStatus::Complete,
        total_required_fields: 0,
        mapped_required_fields: 0,
        total_optional_fields: 0,
        mapped_optional_fields: 0,
        completion_percentage: 100.0,
        missing_required_fields: vec![],
        missing_optional_fields: vec![],
    };

    let usage_info = UsageInfo {
        input_tokens: help_result.usage.input_tokens,
        output_tokens: help_result.usage.output_tokens,
        total_tokens: help_result.usage.total_tokens,
        model: provider.get_model_name().to_string(),
        estimated: help_result.usage.estimated,
    };

    Ok(EnhancedAnalysisResult {
        endpoint_id: "help_capabilities".to_string(),
        endpoint_name: "Help - Available Capabilities".to_string(),
        endpoint_description: "List of available system capabilities and how to use them".to_string(),
        verb: "GET".to_string(),
        base: "help".to_string(),
        path: "/capabilities".to_string(),
        essential_path: "/capabilities".to_string(),
        api_group_id: "help".to_string(),
        api_group_name: "Help System".to_string(),
        parameters: vec![],
        raw_json: serde_json::json!({
            "type": "help_request",
            "response": help_result.content,
            "intent": "help_request",
            "capabilities_count": enhanced_endpoints.len()
        }),
        conversation_id,
        matching_info,
        user_prompt: None,
        total_input_tokens: usage_info.input_tokens,
        total_output_tokens: usage_info.output_tokens,
        usage: usage_info,
        intent: IntentType::HelpRequest,
    })
}

async fn create_general_response(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    model: String,
    conversation_id: Option<String>,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let conversational_result = handle_general_question(sentence, provider.clone()).await?;

    let matching_info = MatchingInfo {
        status: MatchingStatus::Complete,
        total_required_fields: 0,
        mapped_required_fields: 0,
        total_optional_fields: 0,
        mapped_optional_fields: 0,
        completion_percentage: 100.0,
        missing_required_fields: vec![],
        missing_optional_fields: vec![],
    };

    let usage_info = UsageInfo {
        input_tokens: conversational_result.usage.input_tokens,
        output_tokens: conversational_result.usage.output_tokens,
        total_tokens: conversational_result.usage.total_tokens,
        model,
        estimated: conversational_result.usage.estimated,
    };

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
        parameters: vec![],
        raw_json: serde_json::json!({
            "type": "general_conversation",
            "response": conversational_result.content,
            "intent": "general_question"
        }),
        conversation_id,
        matching_info,
        user_prompt: None,
        total_input_tokens: usage_info.input_tokens,
        total_output_tokens: usage_info.output_tokens,
        usage: usage_info,
        intent: IntentType::GeneralQuestion,
    })
}
