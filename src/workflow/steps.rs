use super::find_closest_endpoint::find_closest_endpoint;

use crate::models::{ConfigFile, EnhancedEndpoint};
use crate::workflow::context::WorkflowContext;
// use crate::workflow::find_closest_endpoint::find_closest_endpoint_pure_llm;
use crate::workflow::sentence_to_json::sentence_to_json_structured;
use std::{error::Error, sync::Arc};

// use async_trait::async_trait;

use crate::models::Endpoint;

#[allow(dead_code)]
pub struct JsonGenerationStep {}

// Trait defining a workflow step
#[async_trait]
pub trait WorkflowStep: Send + Sync {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn name(&self) -> &'static str;
}

#[async_trait]
impl WorkflowStep for JsonGenerationStep {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Check if we have an enhanced endpoint to work with
        let json_output = if let Some(enhanced_endpoints) = &context.enhanced_endpoints {
            if let Some(endpoint_id) = &context.endpoint_id {
                // Find the specific endpoint
                if let Some(endpoint) = enhanced_endpoints.iter().find(|e| e.id == *endpoint_id) {
                    // Use structured extraction with known endpoint
                    sentence_to_json_structured(
                        &context.sentence,
                        endpoint,
                        context.provider.clone(),
                    )
                    .await?
                } else {
                    // Fallback to general extraction
                    crate::workflow::sentence_to_json::sentence_to_json(
                        &context.sentence,
                        context.provider.clone(),
                    )
                    .await?
                }
            } else {
                // No specific endpoint selected yet, use general extraction
                crate::workflow::sentence_to_json::sentence_to_json(
                    &context.sentence,
                    context.provider.clone(),
                )
                .await?
            }
        } else {
            // No enhanced endpoints available, use general extraction
            crate::workflow::sentence_to_json::sentence_to_json(
                &context.sentence,
                context.provider.clone(),
            )
            .await?
        };

        context.json_output = Some(json_output);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "adaptive_json_generation"
    }
}

pub struct EndpointMatchingStep {
    pub config: Arc<ConfigFile>,
}

#[async_trait]
impl WorkflowStep for EndpointMatchingStep {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let endpoint =
            find_closest_endpoint(&self.config, &context.sentence, context.provider.clone())
                .await?;
        context.matched_endpoint = Some(endpoint);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "endpoint_matching"
    }
}

// use crate::models::EndpointParameter;
use async_trait::async_trait;

// Workflow configuration loaded from YAML
pub struct FieldMatchingStep {}

#[async_trait]
impl WorkflowStep for FieldMatchingStep {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Use enhanced endpoints if available, otherwise convert regular endpoints
        let enhanced_endpoints = if let Some(enhanced) = &context.enhanced_endpoints {
            enhanced.clone()
        } else {
            // Convert regular endpoints to enhanced format for compatibility
            let config = context
                .endpoints_config
                .as_ref()
                .ok_or("Endpoints config not loaded")?;

            config
                .endpoints
                .iter()
                .map(|e| EnhancedEndpoint {
                    id: e.id.clone(),
                    name: e.text.clone(),
                    text: e.text.clone(),
                    description: e.description.clone(),
                    verb: "POST".to_string(),
                    base: "".to_string(),
                    path: format!("/{}", e.id),
                    essential_path: format!("/{}", e.id),
                    api_group_id: "default".to_string(),
                    api_group_name: "Default Group".to_string(),
                    parameters: e.parameters.clone(),
                })
                .collect()
        };

        // Use the new pure LLM matching
        let selected_endpoint =
            crate::workflow::find_closest_endpoint::find_closest_endpoint_pure_llm(
                &enhanced_endpoints,
                &context.sentence,
                context.provider.clone(),
            )
            .await?;

        context.endpoint_id = Some(selected_endpoint.id.clone());
        context.endpoint_description = Some(selected_endpoint.description.clone());

        // Convert to regular endpoint for compatibility
        context.matched_endpoint = Some(Endpoint {
            id: selected_endpoint.id.clone(),
            text: selected_endpoint.text.clone(),
            description: selected_endpoint.description.clone(),
            parameters: selected_endpoint.parameters.clone(),
        });

        Ok(())
    }

    fn name(&self) -> &'static str {
        "pure_llm_endpoint_matching"
    }
}
