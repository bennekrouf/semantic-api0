use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use tracing::warn;

#[derive(Debug, Deserialize)]
struct PromptVersion {
    template: String,
}

#[derive(Debug, Deserialize)]
struct PromptVersions {
    versions: HashMap<String, PromptVersion>,
    default_version: String,
}

#[derive(Debug, Deserialize)]
struct PromptConfig {
    prompts: HashMap<String, PromptVersions>,
}

pub struct PromptManager {
    config: PromptConfig,
}

impl PromptManager {
    pub async fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let prompts_path = env::var("PROMPTS_PATH").unwrap_or_else(|_| "prompts.yaml".to_string());
        let config_str = tokio::fs::read_to_string(&prompts_path).await?;
        let config: PromptConfig = serde_yaml::from_str(&config_str)?;
        Ok(Self { config })
    }

    pub fn format_extract_followup_parameters_with_mapping(
        &self,
        sentence: &str,
        available_parameters: &str,
        version: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let template = self
            .get_prompt("extract_followup_parameters_mapping", version)
            .ok_or("extract_followup_parameters_mapping prompt not found in prompts.yaml")?;

        Ok(template
            .replace("{sentence}", sentence)
            .replace("{available_parameters}", available_parameters))
    }

    pub fn format_help_response_with_language(
        &self,
        sentence: &str,
        endpoints_list: &str,
        detected_language: &str,
        version: Option<&str>,
    ) -> String {
        let template = self
            .get_prompt("help_response", version)
            .unwrap_or_default();

        template
            .replace("{sentence}", sentence)
            .replace("{endpoints_list}", endpoints_list)
            .replace("{detected_language}", detected_language)
    }

    /// Gets a prompt template by name and optional version
    pub fn get_prompt(&self, name: &str, version: Option<&str>) -> Option<&str> {
        let prompt_versions = self.config.prompts.get(name)?;

        let version_key = version.unwrap_or(&prompt_versions.default_version);

        match prompt_versions.versions.get(version_key) {
            Some(version) => Some(&version.template),
            None => {
                warn!(
                    "Prompt version {} not found for {}, falling back to default",
                    version_key, name
                );
                prompt_versions
                    .versions
                    .get(&prompt_versions.default_version)
                    .map(|v| &v.template)
                    .map(|x| x.as_str())
            }
        }
    }

    pub fn format_intent_classification(
        &self,
        sentence: &str,
        endpoints_list: &str,
        version: Option<&str>,
    ) -> String {
        let template = self
            .get_prompt("intent_classification", version)
            .unwrap_or_default();

        template
            .replace("{sentence}", sentence)
            .replace("{endpoints_list}", endpoints_list)
    }

    pub fn format_find_endpoint_v2(
        &self,
        input_sentence: &str,
        endpoints_list: &str,
        version: Option<&str>,
    ) -> String {
        let template = self
            .get_prompt("find_endpoint", version)
            .unwrap_or_default();

        template
            .replace("{input_sentence}", input_sentence)
            .replace("{endpoints_list}", endpoints_list)
    }

    pub fn format_sentence_to_json(&self, sentence: &str, version: Option<&str>) -> String {
        let template = self
            .get_prompt("sentence_to_json", version)
            .unwrap_or_default();

        template.replace("{sentence}", sentence)
    }

    pub fn format_sentence_to_json_v2(
        &self,
        sentence: &str,
        endpoint_description: &str,
        required_params: &str,
        optional_params: &str,
        version: Option<&str>,
    ) -> String {
        let template = self
            .get_prompt("sentence_to_json", version)
            .unwrap_or_default();

        template
            .replace("{sentence}", sentence)
            .replace("{endpoint_description}", endpoint_description)
            .replace("{required_params}", required_params)
            .replace("{optional_params}", optional_params)
    }
}
