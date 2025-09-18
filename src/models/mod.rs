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

#[derive(Debug, Serialize)]
pub struct EnhancedAnalysisResult {
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub endpoint_description: String,
    pub verb: String,
    pub base: String,
    pub path: String,
    pub essential_path: String,
    pub api_group_id: String,
    pub api_group_name: String,
    pub parameters: Vec<ParameterMatch>,
    pub raw_json: serde_json::Value,
    pub conversation_id: Option<String>,
    pub matching_info: MatchingInfo,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
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
        let mut total_required = 0;
        let mut mapped_required = 0;
        let mut total_optional = 0;
        let mut mapped_optional = 0;
        let mut missing_required = Vec::new();
        let mut missing_optional = Vec::new();

        for endpoint_param in endpoint_params {
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
}
