use crate::app_log;
use crate::utils::token_calculator::EnhancedTokenCalculator;
use crate::workflow::sentence_to_json::sentence_to_json;
use crate::workflow::WorkflowContext;
use crate::workflow::WorkflowStep;
use async_trait::async_trait;
use std::error::Error;

pub struct JsonGenerationStep;

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
            "JSON generation step added {} input tokens, {} output tokens",
            step_usage.input_tokens,
            step_usage.output_tokens
        );

        Ok(())
    }
    fn name(&self) -> &'static str {
        "json_generation"
    }
}
