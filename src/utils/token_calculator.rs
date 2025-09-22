// src/utils/token_calculator.rs
use std::collections::HashMap;
use tracing::debug;

pub struct EnhancedTokenCalculator {
    // More accurate token estimation ratios per provider
    provider_rates: HashMap<String, TokenRatio>,
}

#[derive(Clone, Debug)]
struct TokenRatio {
    chars_per_token: f32,
    words_per_token: f32,
    // Different languages have different token densities
    language_multipliers: HashMap<String, f32>,
}

impl EnhancedTokenCalculator {
    pub fn new() -> Self {
        let mut provider_rates = HashMap::new();

        // Cohere token ratios (based on empirical testing)
        provider_rates.insert(
            "cohere".to_string(),
            TokenRatio {
                chars_per_token: 3.8,  // Cohere is slightly more efficient than GPT
                words_per_token: 0.75, // ~1.33 tokens per word
                language_multipliers: {
                    let mut lang_map = HashMap::new();
                    lang_map.insert("en".to_string(), 1.0);
                    lang_map.insert("fr".to_string(), 1.15); // French is slightly more token-heavy
                    lang_map.insert("es".to_string(), 1.1);
                    lang_map.insert("de".to_string(), 1.2);
                    lang_map
                },
            },
        );

        // Claude token ratios
        provider_rates.insert(
            "claude".to_string(),
            TokenRatio {
                chars_per_token: 4.1,
                words_per_token: 0.73,
                language_multipliers: {
                    let mut lang_map = HashMap::new();
                    lang_map.insert("en".to_string(), 1.0);
                    lang_map.insert("fr".to_string(), 1.12);
                    lang_map.insert("es".to_string(), 1.08);
                    lang_map.insert("de".to_string(), 1.18);
                    lang_map
                },
            },
        );

        // DeepSeek token ratios (similar to OpenAI)
        provider_rates.insert(
            "deepseek".to_string(),
            TokenRatio {
                chars_per_token: 4.0,
                words_per_token: 0.75,
                language_multipliers: {
                    let mut lang_map = HashMap::new();
                    lang_map.insert("en".to_string(), 1.0);
                    lang_map.insert("fr".to_string(), 1.13);
                    lang_map.insert("es".to_string(), 1.09);
                    lang_map.insert("de".to_string(), 1.16);
                    lang_map
                },
            },
        );

        Self { provider_rates }
    }

    /// Estimate tokens with enhanced accuracy using multiple methods
    pub fn estimate_tokens_enhanced(
        &self,
        text: &str,
        provider: &str,
        language: Option<&str>,
    ) -> u32 {
        let ratio = self
            .provider_rates
            .get(provider)
            .unwrap_or_else(|| self.provider_rates.get("claude").unwrap());

        let lang_multiplier = language
            .and_then(|lang| ratio.language_multipliers.get(lang))
            .unwrap_or(&1.0);

        // Method 1: Character-based estimation
        let char_estimate = (text.len() as f32 / ratio.chars_per_token * lang_multiplier) as u32;

        // Method 2: Word-based estimation
        let word_count = text.split_whitespace().count();
        let word_estimate = (word_count as f32 / ratio.words_per_token * lang_multiplier) as u32;

        // Method 3: Combined approach (weighted average)
        let combined_estimate =
            ((char_estimate as f32 * 0.6) + (word_estimate as f32 * 0.4)) as u32;

        debug!(
            "Token estimation for {} ({}): chars={}, words={}, combined={}, text_len={}",
            provider,
            language.unwrap_or("en"),
            char_estimate,
            word_estimate,
            combined_estimate,
            text.len()
        );

        // Use combined estimate, with minimum of 1 token for non-empty text
        if text.trim().is_empty() {
            0
        } else {
            combined_estimate.max(1)
        }
    }

    /// Detect language from text content (simple heuristic)
    pub fn detect_language(&self, text: &str) -> &str {
        let text_lower = text.to_lowercase();

        // Simple language detection based on common words
        if text_lower.contains("the ")
            || text_lower.contains(" and ")
            || text_lower.contains(" is ")
        {
            "en"
        } else if text_lower.contains(" le ")
            || text_lower.contains(" la ")
            || text_lower.contains(" et ")
            || text_lower.contains(" pour ")
            || text_lower.contains(" avec ")
        {
            "fr"
        } else if text_lower.contains(" el ")
            || text_lower.contains(" la ")
            || text_lower.contains(" y ")
        {
            "es"
        } else if text_lower.contains(" der ")
            || text_lower.contains(" die ")
            || text_lower.contains(" und ")
        {
            "de"
        } else {
            "en" // Default to English
        }
    }

    /// Calculate tokens for both input and output with context
    pub fn calculate_usage(
        &self,
        input_text: &str,
        output_text: &str,
        provider: &str,
    ) -> crate::models::providers::token_counter::TokenUsage {
        let input_language = self.detect_language(input_text);
        let output_language = self.detect_language(output_text);

        let input_tokens =
            self.estimate_tokens_enhanced(input_text, provider, Some(input_language));
        let output_tokens =
            self.estimate_tokens_enhanced(output_text, provider, Some(output_language));

        debug!(
            "Enhanced token calculation for {}: input={} tokens ({}), output={} tokens ({})",
            provider, input_tokens, input_language, output_tokens, output_language
        );

        crate::models::providers::token_counter::TokenUsage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
            estimated: true,
        }
    }

    /// Update estimation based on actual API response (for future calibration)
    pub fn calibrate_from_actual(&mut self, provider: &str, text: &str, actual_tokens: u32) {
        if let Some(ratio) = self.provider_rates.get_mut(provider) {
            if !text.is_empty() && actual_tokens > 0 {
                let actual_chars_per_token = text.len() as f32 / actual_tokens as f32;
                // Exponential moving average to gradually adjust
                ratio.chars_per_token = ratio.chars_per_token * 0.9 + actual_chars_per_token * 0.1;
                debug!(
                    "Calibrated {} chars_per_token to {:.2} based on actual usage",
                    provider, ratio.chars_per_token
                );
            }
        }
    }
}

impl Default for EnhancedTokenCalculator {
    fn default() -> Self {
        Self::new()
    }
}
