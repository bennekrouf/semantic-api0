pub mod config;
pub mod providers;

pub use providers::ModelsConfig;
use serde::{Deserialize, Serialize};

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
        use std::collections::HashSet; // Move import here

        let mut total_required = 0;
        let mut mapped_required = 0;
        let mut total_optional = 0;
        let mut mapped_optional = 0;
        let mut missing_required = Vec::new();
        let mut missing_optional = Vec::new();

        // Use a HashSet to track processed parameter names to avoid duplicates
        let mut processed_params = HashSet::new();

        for endpoint_param in endpoint_params {
            // Skip if we've already processed this parameter name
            if processed_params.contains(&endpoint_param.name) {
                continue;
            }
            processed_params.insert(endpoint_param.name.clone());

            let is_required = endpoint_param.required.unwrap_or(false);
            let is_mapped = parameters
                .iter()
                .any(|p| p.name == endpoint_param.name && p.value.is_some());

            if is_required {
                total_required += 1;
                if is_mapped {
                    mapped_required += 1;
                } else {
                    missing_required.push(MissingField {
                        name: endpoint_param.name.clone(),
                        description: endpoint_param.description.clone(),
                    });
                }
            } else {
                total_optional += 1;
                if is_mapped {
                    mapped_optional += 1;
                } else {
                    missing_optional.push(MissingField {
                        name: endpoint_param.name.clone(),
                        description: endpoint_param.description.clone(),
                    });
                }
            }
        }

        let completion_percentage = if total_required > 0 {
            (mapped_required as f32 / total_required as f32) * 100.0
        } else {
            100.0 // No required fields means 100% complete
        };

        let status = if total_required == 0 || mapped_required == total_required {
            MatchingStatus::Complete
        } else if mapped_required > 0 {
            MatchingStatus::Partial
        } else {
            MatchingStatus::Incomplete
        };

        Self {
            status,
            total_required_fields: total_required,
            mapped_required_fields: mapped_required,
            total_optional_fields: total_optional,
            mapped_optional_fields: mapped_optional,
            completion_percentage,
            missing_required_fields: missing_required,
            missing_optional_fields: missing_optional,
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
