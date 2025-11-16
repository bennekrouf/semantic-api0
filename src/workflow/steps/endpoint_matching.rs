use crate::app_log;
use crate::utils::token_calculator::EnhancedTokenCalculator;
use crate::workflow::find_closest_endpoint::find_closest_endpoint;
use crate::workflow::WorkflowContext;
use crate::workflow::WorkflowStep;
use async_trait::async_trait;
use std::error::Error;

pub struct EndpointMatchingStep;

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
        let enhanced_calculator = EnhancedTokenCalculator::new();
        let step_usage = enhanced_calculator.calculate_usage(
            &context.sentence,
            "",
            context.provider.get_model_name(),
        );

        // Add tokens to context
        context.total_input_tokens += step_usage.input_tokens;
        context.total_output_tokens += step_usage.output_tokens;

        app_log!(
            debug,
            "Endpoint matching step added {} input tokens, {} output tokens",
            step_usage.input_tokens,
            step_usage.output_tokens
        );

        Ok(())
    }
    fn name(&self) -> &'static str {
        "endpoint_matching"
    }
}
