use crate::app_log;
use crate::models::EndpointParameter;
use crate::utils::path_params::extract_path_params_from_path;
use crate::workflow::WorkflowContext;
use crate::workflow::WorkflowStep;
use async_trait::async_trait;
use std::error::Error;

pub struct PathParameterExtractionStep;

#[async_trait]
impl WorkflowStep for PathParameterExtractionStep {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        app_log!(debug, "Step 3: Extracting path parameters");

        let endpoint_id = context
            .endpoint_id
            .as_ref()
            .ok_or("Endpoint ID not available")?
            .clone();

        let enhanced_endpoints = context
            .enhanced_endpoints
            .as_ref()
            .ok_or("Enhanced endpoints not available")?;

        let enhanced_endpoint = enhanced_endpoints
            .iter()
            .find(|e| &e.id == &endpoint_id)
            .ok_or("Enhanced endpoint not found")?
            .clone();

        app_log!(debug, "Processing path: {}", enhanced_endpoint.path);

        // Extract path parameters
        let path_parameters = extract_path_params_from_path(&enhanced_endpoint.path)?;
        app_log!(debug, "Path parameters found: {:?}", path_parameters);

        // Initialize parameters with existing endpoint parameters
        let mut parameters: Vec<EndpointParameter> = enhanced_endpoint.parameters.clone();

        // Add path parameters that aren't already in the endpoint definition
        if let Some(path_params) = path_parameters {
            app_log!(
                debug,
                "Found {} path parameters to process",
                path_params.len()
            );
            for (param_name, _param_placeholder) in path_params {
                app_log!(debug, "Processing path parameter: {}", param_name);
                if !parameters.iter().any(|p| p.name == param_name) {
                    app_log!(debug, "Adding missing path parameter: {}", param_name);
                    parameters.push(EndpointParameter {
                        name: param_name.clone(),
                        description: format!("URL path parameter: {}", param_name),
                        semantic_value: None,
                        alternatives: None,
                        required: Some(true),
                    });
                } else {
                    app_log!(debug, "Skipping existing path parameter: {}", param_name);
                }
            }
        }

        context.parameters = parameters;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "path_parameter_extraction"
    }
}
