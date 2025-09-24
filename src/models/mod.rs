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

impl MatchingInfo {
    pub fn compute(parameters: &[ParameterMatch], endpoint_params: &[EndpointParameter]) -> Self {
        use tracing::debug;

        debug!("=== MatchingInfo Debug ===");
        debug!("Input parameters: {:#?}", parameters);
        debug!("Endpoint parameters: {:#?}", endpoint_params);

        // Helper function to check if a parameter has a valid value
        fn has_valid_value(param: &ParameterMatch) -> bool {
            param
                .value
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
        }

        // Helper function to find matching parameter by name
        fn find_matched_param<'a>(
            param_name: &str,
            parameters: &'a [ParameterMatch],
        ) -> Option<&'a ParameterMatch> {
            parameters.iter().find(|p| p.name == param_name)
        }

        // Separate endpoint parameters by requirement status
        let (required_params, optional_params): (Vec<_>, Vec<_>) = endpoint_params
            .iter()
            .partition(|ep| ep.required.unwrap_or(false));

        // Process required parameters
        let (mapped_required, missing_required): (Vec<_>, Vec<_>) = required_params
            .iter()
            .map(|ep| {
                let matched_param = find_matched_param(&ep.name, parameters);
                let is_mapped = matched_param.map(has_valid_value).unwrap_or(false);

                if is_mapped {
                    (Some(ep), None)
                } else {
                    (
                        None,
                        Some(MissingField {
                            name: ep.name.clone(),
                            description: ep.description.clone(),
                        }),
                    )
                }
            })
            .unzip();

        // Process optional parameters
        let (mapped_optional, missing_optional): (Vec<_>, Vec<_>) = optional_params
            .iter()
            .map(|ep| {
                let matched_param = find_matched_param(&ep.name, parameters);
                let is_mapped = matched_param.map(has_valid_value).unwrap_or(false);

                if is_mapped {
                    (Some(ep), None)
                } else {
                    (
                        None,
                        Some(MissingField {
                            name: ep.name.clone(),
                            description: ep.description.clone(),
                        }),
                    )
                }
            })
            .unzip();

        // Extract counts and lists
        let total_required_fields = required_params.len();
        let mapped_required_fields = mapped_required.into_iter().flatten().count();
        let missing_required_fields: Vec<MissingField> =
            missing_required.into_iter().flatten().collect();

        let total_optional_fields = optional_params.len();
        let mapped_optional_fields = mapped_optional.into_iter().flatten().count();
        let missing_optional_fields: Vec<MissingField> =
            missing_optional.into_iter().flatten().collect();

        // Calculate completion percentage
        let completion_percentage = if total_required_fields > 0 {
            (mapped_required_fields as f32 / total_required_fields as f32) * 100.0
        } else {
            100.0 // No required fields means 100% complete
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
