pub mod config;
pub mod providers;

pub use providers::ModelsConfig;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct GenerateRequest {
    pub model: String,
    pub prompt: String,
    pub stream: bool,
    pub format: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
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
}

#[derive(Debug, Serialize, Clone)]
pub struct ParameterMatch {
    pub name: String,
    pub description: String,
    pub value: Option<String>,
}
