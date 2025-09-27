pub mod config;
pub mod providers;

pub use providers::ModelsConfig;
use serde::{Deserialize, Serialize};

use crate::workflow::classify_intent::IntentType;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MissingField {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct OllamaResponse {
    //pub response: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Endpoint {
    pub id: String,
    pub text: String,
    pub description: String,
    pub parameters: Vec<EndpointParameter>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EndpointParameter {
    pub name: String,
    pub description: String,
    pub required: Option<bool>,
    pub alternatives: Option<Vec<String>>,
    pub semantic_value: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigFile {
    pub endpoints: Vec<Endpoint>,
}

impl ConfigFile {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnhancedEndpoint {
    pub id: String,
    pub name: String,
    pub text: String,
    pub description: String,
    pub verb: String,
    pub base: String,
    pub path: String,
    pub essential_path: String,
    pub api_group_id: String,
    pub api_group_name: String,
    pub parameters: Vec<EndpointParameter>,
}

#[derive(Debug, Serialize, Clone)]
pub struct UsageInfo {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub model: String,
    pub estimated: bool,
}

impl From<&crate::models::providers::token_counter::TokenUsage> for UsageInfo {
    fn from(usage: &crate::models::providers::token_counter::TokenUsage) -> Self {
        Self {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            model: "unknown".to_string(), // Will be set by caller
            estimated: usage.estimated,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EnhancedAnalysisResult {
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub endpoint_description: String,
    pub verb: String,
    pub base: String,
    pub path: String,
    pub user_prompt: Option<String>,
    pub essential_path: String,
    pub api_group_id: String,
    pub api_group_name: String,
    pub parameters: Vec<ParameterMatch>,
    pub raw_json: serde_json::Value,
    pub conversation_id: Option<String>,
    pub matching_info: MatchingInfo,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    pub usage: UsageInfo,
    pub intent: IntentType,
}

#[derive(Debug, Serialize, Clone)]
pub struct ParameterMatch {
    pub name: String,
    pub description: String,
    pub value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MatchingStatus {
    Complete,   // All required fields mapped
    Partial,    // Some required fields missing
    Incomplete, // Many/most required fields missing
}

#[derive(Debug, Serialize, Clone)]
pub struct MatchingInfo {
    pub status: MatchingStatus,
    pub total_required_fields: usize,
    pub mapped_required_fields: usize,
    pub total_optional_fields: usize,
    pub mapped_optional_fields: usize,
    pub completion_percentage: f32,
    pub missing_required_fields: Vec<MissingField>,
    pub missing_optional_fields: Vec<MissingField>,
}

pub fn debug_parameter_matches(
    parameter_matches: &[ParameterMatch],
    endpoint_params: &[EndpointParameter],
) {
    use tracing::{debug, warn};

    debug!("=== DEBUGGING PARAMETER CONSTRUCTION ===");

    debug!(
        "ParameterMatch objects ({} total):",
        parameter_matches.len()
    );
    for (i, param) in parameter_matches.iter().enumerate() {
        debug!("  [{}] name: '{}', value: {:?}", i, param.name, param.value);
    }

    debug!(
        "EndpointParameter objects ({} total):",
        endpoint_params.len()
    );
    let mut param_counts = std::collections::HashMap::new();
    for (i, param) in endpoint_params.iter().enumerate() {
        debug!(
            "  [{}] name: '{}', required: {:?}, desc: '{}'",
            i, param.name, param.required, param.description
        );

        // Count duplicates
        *param_counts.entry(param.name.clone()).or_insert(0) += 1;
    }

    // Check for duplicates in endpoint params
    for (name, count) in param_counts {
        if count > 1 {
            warn!(
                "DUPLICATE EndpointParameter: '{}' appears {} times",
                name, count
            );
        }
    }

    debug!("=== END DEBUGGING ===");
}

impl MatchingInfo {
    pub fn compute(parameters: &[ParameterMatch], endpoint_params: &[EndpointParameter]) -> Self {
        use std::collections::HashMap;
        use tracing::{debug, warn};

        debug!(
            "MatchingInfo::compute called with {} ParameterMatch and {} EndpointParameter",
            parameters.len(),
            endpoint_params.len()
        );

        // Log what we receive
        for param in parameters {
            debug!("ParameterMatch: '{}' = {:?}", param.name, param.value);
        }

        for param in endpoint_params {
            debug!(
                "EndpointParameter: '{}' (required: {:?})",
                param.name, param.required
            );
        }

        // Helper function to check if a parameter has a valid value
        fn has_valid_value(param: &ParameterMatch) -> Option<bool> {
            param.value.as_ref().map(|v| !v.trim().is_empty())
        }

        // Deduplicate endpoint parameters by name (keep first occurrence)
        let mut unique_params: HashMap<String, &EndpointParameter> = HashMap::new();
        let mut duplicates_found = false;

        for param in endpoint_params {
            if unique_params.contains_key(&param.name) {
                warn!(
                    "DUPLICATE found: parameter '{}' appears multiple times",
                    param.name
                );
                duplicates_found = true;
            }
            unique_params.entry(param.name.clone()).or_insert(param);
        }

        if duplicates_found {
            warn!(
                "Duplicates were found and removed. Unique parameters: {:?}",
                unique_params.keys().collect::<Vec<_>>()
            );
        }

        debug!(
            "After deduplication: {} unique parameters",
            unique_params.len()
        );

        // Create lookup map for parameter matches
        let param_lookup: HashMap<String, &ParameterMatch> =
            parameters.iter().map(|p| (p.name.clone(), p)).collect();

        debug!(
            "Parameter lookup created with {} entries",
            param_lookup.len()
        );

        // Single pass: process each unique endpoint parameter exactly once
        let (required_results, optional_results): (Vec<_>, Vec<_>) = unique_params
            .values()
            .map(|endpoint_param| {
                let is_required = endpoint_param.required.unwrap_or(false);
                let matched_param = param_lookup.get(&endpoint_param.name);
                let has_value = matched_param
                    .and_then(|p| has_valid_value(p))
                    .unwrap_or(false);

                debug!(
                    "Processing '{}': required={}, matched={}, has_value={}",
                    endpoint_param.name,
                    is_required,
                    matched_param.is_some(),
                    has_value
                );

                let result = ParameterResult {
                    endpoint_param,
                    has_value,
                };

                if is_required {
                    debug!("  -> Adding to REQUIRED list");
                    (Some(result), None)
                } else {
                    debug!("  -> Adding to OPTIONAL list");
                    (None, Some(result))
                }
            })
            .unzip();

        // Flatten the results
        let required_results: Vec<ParameterResult> =
            required_results.into_iter().flatten().collect();
        let optional_results: Vec<ParameterResult> =
            optional_results.into_iter().flatten().collect();

        debug!("Required parameters: {} total", required_results.len());
        for result in &required_results {
            debug!(
                "  Required: '{}' has_value={}",
                result.endpoint_param.name, result.has_value
            );
        }

        debug!("Optional parameters: {} total", optional_results.len());
        for result in &optional_results {
            debug!(
                "  Optional: '{}' has_value={}",
                result.endpoint_param.name, result.has_value
            );
        }

        // Calculate counts and missing lists
        let total_required_fields = required_results.len();
        let mapped_required_fields = required_results.iter().filter(|r| r.has_value).count();
        let missing_required_fields: Vec<MissingField> = required_results
            .iter()
            .filter(|r| !r.has_value)
            .map(|r| MissingField {
                name: r.endpoint_param.name.clone(),
                description: r.endpoint_param.description.clone(),
            })
            .collect();

        let total_optional_fields = optional_results.len();
        let mapped_optional_fields = optional_results.iter().filter(|r| r.has_value).count();
        let missing_optional_fields: Vec<MissingField> = optional_results
            .iter()
            .filter(|r| !r.has_value)
            .map(|r| MissingField {
                name: r.endpoint_param.name.clone(),
                description: r.endpoint_param.description.clone(),
            })
            .collect();

        debug!("FINAL RESULTS:");
        debug!(
            "  Required: {}/{} mapped",
            mapped_required_fields, total_required_fields
        );
        debug!(
            "  Optional: {}/{} mapped",
            mapped_optional_fields, total_optional_fields
        );
        debug!(
            "  Missing required: {:?}",
            missing_required_fields
                .iter()
                .map(|f| &f.name)
                .collect::<Vec<_>>()
        );
        debug!(
            "  Missing optional: {:?}",
            missing_optional_fields
                .iter()
                .map(|f| &f.name)
                .collect::<Vec<_>>()
        );

        // Calculate completion percentage
        let completion_percentage = if total_required_fields > 0 {
            (mapped_required_fields as f32 / total_required_fields as f32) * 100.0
        } else {
            100.0
        };

        // Determine status
        let status =
            if total_required_fields == 0 || mapped_required_fields == total_required_fields {
                MatchingStatus::Complete
            } else if mapped_required_fields > 0 {
                MatchingStatus::Partial
            } else {
                MatchingStatus::Incomplete
            };

        Self {
            status,
            total_required_fields,
            mapped_required_fields,
            total_optional_fields,
            mapped_optional_fields,
            completion_percentage,
            missing_required_fields,
            missing_optional_fields,
        }
    }

    /// Generate a natural language prompt for missing fields
    pub fn generate_user_prompt(&self, endpoint_name: &str) -> Option<String> {
        if self.missing_required_fields.is_empty() {
            return None;
        }

        let missing_count = self.missing_required_fields.len();

        match missing_count {
            1 => {
                let field = &self.missing_required_fields[0];
                Some(format!(
                    "To proceed with {}, I need one more piece of information: {}. Could you please provide that?",
                    endpoint_name.to_lowercase(),
                    Self::format_field_request(&field.name, &field.description)
                ))
            }
            2 => {
                let field1 = &self.missing_required_fields[0];
                let field2 = &self.missing_required_fields[1];
                Some(format!(
                    "To complete your {} request, I need {} and {}. Could you provide these details?",
                    endpoint_name.to_lowercase(),
                    Self::format_field_request(&field1.name, &field1.description),
                    Self::format_field_request(&field2.name, &field2.description)
                ))
            }
            _ => {
                let field_list: Vec<String> = self
                    .missing_required_fields
                    .iter()
                    .map(|f| Self::format_field_request(&f.name, &f.description))
                    .collect();

                let (initial_fields, last_field) = field_list.split_at(field_list.len() - 1);

                Some(format!(
                    "To process your {} request, I need a few more details: {}, and {}. Could you provide this information?",
                    endpoint_name.to_lowercase(),
                    initial_fields.join(", "),
                    last_field[0]
                ))
            }
        }
    }

    fn format_field_request(field_name: &str, field_description: &str) -> String {
        // Convert snake_case to natural language
        let natural_name = field_name.replace('_', " ").replace('-', " ");

        // Use description if it's more descriptive than the field name
        if field_description.len() > natural_name.len() + 5
            && !field_description
                .to_lowercase()
                .starts_with("missing parameter")
        {
            field_description.to_lowercase()
        } else {
            format!("the {}", natural_name)
        }
    }
}

// Helper struct for cleaner processing
struct ParameterResult<'a> {
    endpoint_param: &'a EndpointParameter,
    has_value: bool,
}
