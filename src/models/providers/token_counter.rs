// src/models/providers/token_counter.rs
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub estimated: bool, // Whether tokens were estimated or from API
}

pub struct TokenCounter {
    model_rates: HashMap<String, f32>, // tokens per character ratio
}

impl TokenCounter {
    pub fn new() -> Self {
        let mut model_rates = HashMap::new();

        // Default estimation: ~4 chars per token (GPT standard)
        model_rates.insert("default".to_string(), 0.25);

        // Add known models if you want better estimates
        model_rates.insert("claude".to_string(), 0.24);
        model_rates.insert("cohere".to_string(), 0.25);
        model_rates.insert("gpt".to_string(), 0.25);

        Self { model_rates }
    }

    pub fn from_response_enhanced(
        &self,
        response_text: &str,
        input_text: &str,
        model: &str,
    ) -> TokenUsage {
        let calculator = crate::utils::token_calculator::EnhancedTokenCalculator::new();
        calculator.calculate_usage(input_text, response_text, model)
    }

    pub fn estimate_tokens(&self, text: &str, model: &str) -> u32 {
        let rate = self
            .model_rates
            .get(model)
            .or_else(|| self.model_rates.get("default"))
            .unwrap_or(&0.25);

        (text.len() as f32 * rate).ceil() as u32
    }

    pub fn from_response(&self, response_text: &str, input_text: &str, model: &str) -> TokenUsage {
        TokenUsage {
            input_tokens: self.estimate_tokens(input_text, model),
            output_tokens: self.estimate_tokens(response_text, model),
            total_tokens: self.estimate_tokens(input_text, model)
                + self.estimate_tokens(response_text, model),
            estimated: true,
        }
    }

    // Try to extract from API response, fallback to estimation
    pub fn from_api_response(
        &self,
        response_json: &serde_json::Value,
        input_text: &str,
        output_text: &str,
        model: &str,
    ) -> TokenUsage {
        // Try common API response formats
        if let Some(usage) = self.try_extract_usage(response_json) {
            usage
        } else {
            self.from_response(output_text, input_text, model)
        }
    }

    fn try_extract_usage(&self, response: &serde_json::Value) -> Option<TokenUsage> {
        // Claude format
        if let Some(usage) = response.get("usage") {
            if let (Some(input), Some(output)) = (
                usage.get("input_tokens").and_then(|v| v.as_u64()),
                usage.get("output_tokens").and_then(|v| v.as_u64()),
            ) {
                return Some(TokenUsage {
                    input_tokens: input as u32,
                    output_tokens: output as u32,
                    total_tokens: (input + output) as u32,
                    estimated: false,
                });
            }
        }

        // OpenAI format
        if let Some(usage) = response.get("usage") {
            if let (Some(prompt), Some(completion), Some(total)) = (
                usage.get("prompt_tokens").and_then(|v| v.as_u64()),
                usage.get("completion_tokens").and_then(|v| v.as_u64()),
                usage.get("total_tokens").and_then(|v| v.as_u64()),
            ) {
                return Some(TokenUsage {
                    input_tokens: prompt as u32,
                    output_tokens: completion as u32,
                    total_tokens: total as u32,
                    estimated: false,
                });
            }
        }

        // Cohere format (if available in meta)
        if let Some(meta) = response.get("meta") {
            if let Some(tokens) = meta.get("tokens") {
                if let (Some(input), Some(output)) = (
                    tokens.get("input_tokens").and_then(|v| v.as_u64()),
                    tokens.get("output_tokens").and_then(|v| v.as_u64()),
                ) {
                    return Some(TokenUsage {
                        input_tokens: input as u32,
                        output_tokens: output as u32,
                        total_tokens: (input + output) as u32,
                        estimated: false,
                    });
                }
            }
        }

        None
    }
}
