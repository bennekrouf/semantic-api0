use crate::analysis::progressive_handler::handle_progressive_followup;
use crate::analysis::response_builders::{
    create_fallback_response, create_general_response, create_help_response,
};
use crate::analysis::retry_logic::analyze_with_retry;
use crate::app_log;
use crate::endpoint_client::get_enhanced_endpoints;
use crate::models::config::load_analysis_config;
use crate::models::providers::ModelProvider;
use crate::models::EnhancedAnalysisResult;
use crate::progressive_matching::{get_database_url, ProgressiveMatchingManager};
use crate::utils::email::validate_email;
use crate::workflow::actions::classify_intent::classify_intent;
use crate::workflow::classify_intent::IntentType;
use std::error::Error;
use std::sync::Arc;

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

    let analysis_config = load_analysis_config().await.unwrap_or_default();

    app_log!(
        info,
        "Starting enhanced sentence analysis with {} retry attempts for: {}",
        analysis_config.retry_attempts,
        sentence
    );

    let api_url_ref = api_url.as_ref().ok_or("No API URL provided")?;

    // STEP 1: PROGRESSIVE MATCHING CHECK (HIGHEST PRIORITY)
    // If we have a conversation_id, check for ongoing requests FIRST
    if let Some(ref conv_id) = conversation_id {
        app_log!(
            info,
            "Checking for ongoing progressive match for conversation: {}",
            conv_id
        );

        if let Ok(db_url) = get_database_url() {
            if let Ok(progressive_manager) = ProgressiveMatchingManager::new(&db_url).await {
                // Check if there's an ongoing incomplete match
                match progressive_manager.get_incomplete_match(conv_id).await {
                    Ok(Some(ongoing_match)) => {
                        app_log!(
                            info,
                            "Found ongoing progressive match for endpoint: {}",
                            ongoing_match.endpoint_id
                        );

                        // Process this as a progressive follow-up
                        match handle_progressive_followup(
                            sentence,
                            conv_id,
                            &ongoing_match,
                            provider.clone(),
                            &progressive_manager,
                            api_url_ref,
                            email,
                        )
                        .await
                        {
                            Ok(progressive_result) => {
                                app_log!(info, "Progressive matching completed successfully");
                                return Ok(progressive_result);
                            }
                            Err(e) => {
                                app_log!(
                                    warn,
                                    "Progressive matching failed: {}, continuing with normal flow",
                                    e
                                );
                                // Continue to normal flow if progressive matching fails
                            }
                        }
                    }
                    Ok(None) => {
                        app_log!(
                            debug,
                            "No ongoing progressive match found for conversation: {}",
                            conv_id
                        );
                    }
                    Err(e) => {
                        app_log!(
                            warn,
                            "Error checking for progressive match: {}, continuing with normal flow",
                            e
                        );
                    }
                }
            }
        }
    }

    // STEP 2: NORMAL FLOW (Intent Classification + Endpoint Matching)
    // Only reached if no progressive match was found or it failed
    app_log!(
        info,
        "No progressive match found, proceeding with normal analysis flow"
    );

    let enhanced_endpoints = get_enhanced_endpoints(api_url_ref, email).await?;
    let endpoint_descriptions: Vec<String> = enhanced_endpoints
        .iter()
        .map(|e| e.description.clone())
        .collect();

    let intent = classify_intent(sentence, &endpoint_descriptions, provider.clone()).await?;

    match intent {
        IntentType::ActionableRequest => {
            app_log!(info, "Processing as NEW actionable request");
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
                        app_log!(
                            warn,
                            "All retries failed, falling back to general question handler: {}",
                            e
                        );
                        create_fallback_response(sentence, provider, model, conversation_id).await
                    } else {
                        Err(e)
                    }
                }
            }
        }

        IntentType::HelpRequest => {
            app_log!(info, "Processing as help request");
            create_help_response(sentence, &enhanced_endpoints, provider, conversation_id).await
        }

        IntentType::GeneralQuestion => {
            app_log!(info, "Processing as general question");
            create_general_response(sentence, provider, model, conversation_id).await
        }
    }
}
