use crate::models::{
    providers::ModelProvider, ConfigFile, Endpoint, EndpointParameter, EnhancedEndpoint,
    ModelsConfig,
};
use serde_json::Value;
use std::sync::Arc;

// Remove the Debug derive since dyn ModelProvider doesn't implement Debug
#[derive(Clone)]
pub struct WorkflowContext {
    // Input
    pub sentence: String,
    pub email: Option<String>,
    // pub conversation_id: Option<String>,
    // Configurations
    pub models_config: Option<ModelsConfig>,
    pub endpoints_config: Option<ConfigFile>,
    pub enhanced_endpoints: Option<Vec<EnhancedEndpoint>>,

    // Processing state
    pub json_output: Option<Value>,
    pub matched_endpoint: Option<Endpoint>,
    pub parameters: Vec<EndpointParameter>,
    pub endpoint_id: Option<String>,
    pub endpoint_description: Option<String>,
    pub provider: Arc<dyn ModelProvider>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

impl WorkflowContext {
    pub fn new(sentence: String, provider: Arc<dyn ModelProvider>) -> Self {
        Self {
            sentence,
            email: None,
            // conversation_id: None,
            provider,
            models_config: None,
            endpoints_config: None,
            json_output: None,
            matched_endpoint: None,
            enhanced_endpoints: None,
            parameters: vec![],
            endpoint_id: None,
            endpoint_description: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    // pub fn add_token_usage(&mut self, usage: &TokenUsage) {
    //     self.total_input_tokens += usage.input_tokens;
    //     self.total_output_tokens += usage.output_tokens;
    // }
}

// Manually implement Debug to handle the provider field
impl std::fmt::Debug for WorkflowContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkflowContext")
            .field("sentence", &self.sentence)
            .field("email", &self.email)
            .field("models_config", &self.models_config)
            .field("endpoints_config", &self.endpoints_config)
            .field("json_output", &self.json_output)
            .field("matched_endpoint", &self.matched_endpoint)
            .field("parameters", &self.parameters)
            .field("endpoint_id", &self.endpoint_id)
            .field("endpoint_description", &self.endpoint_description)
            .field("provider", &"<dyn ModelProvider>")
            .finish()
    }
}
