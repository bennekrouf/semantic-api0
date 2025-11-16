use crate::app_log;
use crate::models::providers::ModelProvider;
use crate::models::EnhancedAnalysisResult;
use crate::models::{MatchingInfo, ParameterMatch, UsageInfo};
use crate::utils::token_calculator::EnhancedTokenCalculator;
use crate::workflow::classify_intent::IntentType;
use crate::workflow::steps::endpoint_matching::EndpointMatchingStep;
use crate::workflow::steps::enhanced_config_loading::EnhancedConfigurationLoadingStep;
use crate::workflow::steps::field_matching::FieldMatchingStep;
use crate::workflow::steps::json_generation::JsonGenerationStep;
use crate::workflow::steps::path_parameter_extraction::PathParameterExtractionStep;
use crate::workflow::{WorkflowConfig, WorkflowEngine};
use std::error::Error;
use std::sync::Arc;

// Retry logic for actionable analysis
pub async fn analyze_with_retry(
    sentence: &str,
    provider: Arc<dyn ModelProvider>,
    api_url: Option<String>,
    email: &str,
    conversation_id: Option<String>,
    retry_attempts: u32,
) -> Result<EnhancedAnalysisResult, Box<dyn Error + Send + Sync>> {
    let mut last_error = None;

    for attempt in 1..=retry_attempts {
        app_log!(
            info,
            "Analysis attempt {}/{} for: {}",
            attempt,
            retry_attempts,
            sentence
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
                app_log!(info, "Analysis succeeded on attempt {}", attempt);
                return Ok(result);
            }
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("No suitable endpoint found")
                    || error_msg.contains("not found in available endpoints")
                {
                    app_log!(
                        warn,
                        "Endpoint matching failed on attempt {}: {}",
                        attempt,
                        error_msg
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
  - name: path_parameter_extraction  # NEW: Extract path parameters
    enabled: true
    retry:
      max_attempts: 1
      delay_ms: 0
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
            "path_parameter_extraction" => {
                // NEW
                engine.register_step(step_config, Arc::new(PathParameterExtractionStep));
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
                app_log!(error, "Unknown step: {}", step_config.name);
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
                .find(|e| context.endpoint_id.as_ref().is_some_and(|id| e.id == *id))
        })
        .ok_or("Enhanced endpoint data not found")?;

    // Build parameter matches from workflow results
    let parameter_matches: Vec<ParameterMatch> = context
        .parameters
        .clone()
        .into_iter()
        .map(|param| ParameterMatch {
            name: param.name,
            description: param.description,
            value: param.semantic_value,
        })
        .collect();
    let matching_info = MatchingInfo::compute(&parameter_matches, &context.parameters);

    // let matching_info = MatchingInfo::compute(&parameter_matches, &enhanced_endpoint.parameters);
    let user_prompt = matching_info.generate_user_prompt(&enhanced_endpoint.name);

    // If workflow didn't track tokens properly, estimate them based on the sentence and response
    let (final_input_tokens, final_output_tokens) = if context.total_output_tokens == 0 {
        app_log!(
            debug,
            "Workflow reported 0 output tokens, estimating output tokens"
        );

        let enhanced_calculator = EnhancedTokenCalculator::new();

        // Use existing input tokens if available, otherwise estimate
        let estimated_input = if context.total_input_tokens > 0 {
            context.total_input_tokens
        } else {
            let sentence_tokens = enhanced_calculator.estimate_tokens_enhanced(
                sentence,
                provider.get_model_name(),
                None,
            );
            sentence_tokens * 3 // Used in ~3 different LLM calls
        };

        // Estimate output tokens based on all the content generated by the workflow
        let mut total_output_content = String::new();

        // Add JSON output content
        if let Some(json_output) = &context.json_output {
            let json_str = serde_json::to_string(json_output).unwrap_or_default();
            total_output_content.push_str(&json_str);
            total_output_content.push(' ');
        }

        // Add endpoint matching result (endpoint ID and description)
        if let Some(endpoint_id) = &context.endpoint_id {
            total_output_content.push_str(endpoint_id);
            total_output_content.push(' ');
        }
        if let Some(desc) = &context.endpoint_description {
            total_output_content.push_str(desc);
            total_output_content.push(' ');
        }

        // Add parameter processing results
        for param in &parameter_matches {
            total_output_content.push_str(&param.name);
            total_output_content.push(' ');
            if let Some(value) = &param.value {
                total_output_content.push_str(value);
                total_output_content.push(' ');
            }
        }

        // Add estimated tokens for LLM reasoning/processing overhead
        let sentence_tokens =
            enhanced_calculator.estimate_tokens_enhanced(sentence, provider.get_model_name(), None);
        let reasoning_overhead = sentence_tokens * 2; // Assume 2x input tokens for reasoning

        let content_tokens = enhanced_calculator.estimate_tokens_enhanced(
            &total_output_content,
            provider.get_model_name(),
            None,
        );
        let estimated_output = content_tokens + reasoning_overhead;

        app_log!(debug, "Output estimation breakdown: content='{}' ({} tokens), reasoning overhead ({} tokens), total output: {}", 
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

    app_log!(
        debug,
        "Final workflow token usage: input={}, output={}, total={}",
        usage_info.input_tokens,
        usage_info.output_tokens,
        usage_info.total_tokens
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
