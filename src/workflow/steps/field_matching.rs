use crate::app_log;
use crate::utils::token_calculator::EnhancedTokenCalculator;
use crate::workflow::match_fields::match_fields_semantic;
use crate::workflow::WorkflowContext;
use crate::workflow::WorkflowStep;
use async_trait::async_trait;
use std::error::Error;

// Reuse existing workflow steps
pub struct FieldMatchingStep;

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

        // Update existing parameters (including path parameters added in previous step) with semantic values
        for param in &mut context.parameters {
            if let Some((_, _, value)) = semantic_results
                .iter()
                .find(|(name, _, _)| name == &param.name)
            {
                param.semantic_value = value.clone();
            }
        }

        // Estimate tokens for field matching step
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
            "Field matching step added {} input tokens, {} output tokens",
            step_usage.input_tokens,
            step_usage.output_tokens
        );

        Ok(())
    }
    fn name(&self) -> &'static str {
        "field_matching"
    }
}
