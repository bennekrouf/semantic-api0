// Create src/models/providers/tracked_provider.rs
use super::{GenerationResult, ModelConfig, ModelProvider};
use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

pub struct TrackedProvider {
    inner: Arc<dyn ModelProvider>,
    total_input_tokens: Arc<Mutex<u32>>,
    total_output_tokens: Arc<Mutex<u32>>,
}

impl TrackedProvider {
    pub fn new(inner: Arc<dyn ModelProvider>) -> Self {
        Self {
            inner,
            total_input_tokens: Arc::new(Mutex::new(0)),
            total_output_tokens: Arc::new(Mutex::new(0)),
        }
    }

    pub async fn get_total_usage(&self) -> (u32, u32) {
        let input = *self.total_input_tokens.lock().await;
        let output = *self.total_output_tokens.lock().await;
        (input, output)
    }

    pub async fn reset_usage(&self) {
        let mut input = self.total_input_tokens.lock().await;
        let mut output = self.total_output_tokens.lock().await;
        *input = 0;
        *output = 0;
    }
}

#[async_trait]
impl ModelProvider for TrackedProvider {
    async fn generate(
        &self,
        prompt: &str,
        config: &ModelConfig,
    ) -> Result<GenerationResult, Box<dyn Error + Send + Sync>> {
        debug!(
            "TrackedProvider: Making LLM call with prompt length: {}",
            prompt.len()
        );

        // Call the inner provider
        let result = self.inner.generate(prompt, config).await?;

        // Track the usage
        {
            let mut input_total = self.total_input_tokens.lock().await;
            let mut output_total = self.total_output_tokens.lock().await;

            *input_total += result.usage.input_tokens;
            *output_total += result.usage.output_tokens;

            debug!(
                "TrackedProvider: Call used {} input / {} output tokens (totals: {} / {})",
                result.usage.input_tokens, result.usage.output_tokens, *input_total, *output_total
            );
        }

        Ok(result)
    }

    fn get_model_name(&self) -> &str {
        self.inner.get_model_name()
    }
}
