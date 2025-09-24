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

    pub fn format_help_response(
        &self,
        sentence: &str,
        endpoints_list: &str,
        version: Option<&str>,
    ) -> String {
        let template = self
            .get_prompt("help_response", version)
            .unwrap_or_default();

        template
            .replace("{sentence}", sentence)
            .replace("{endpoints_list}", endpoints_list)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_prompt_versioning() {
        let manager = PromptManager::new().await.unwrap();

        // Test getting default version
        let prompt = manager.get_prompt("find_endpoint", None);
        assert!(prompt.is_some());

        // Test getting specific version
        let v1_prompt = manager.get_prompt("find_endpoint", Some("v1"));
        assert!(v1_prompt.is_some());

        let v2_prompt = manager.get_prompt("find_endpoint", Some("v2"));
        assert!(v2_prompt.is_some());

        // Test fallback for non-existent version
        let invalid_prompt = manager.get_prompt("find_endpoint", Some("non_existent"));
        assert_eq!(invalid_prompt, manager.get_prompt("find_endpoint", None));

        // Test version listing
        let versions = manager.list_versions("find_endpoint").unwrap();
        assert!(versions.contains(&"v1".to_string()));
        assert!(versions.contains(&"v2".to_string()));
    }

    #[tokio::test]
    async fn test_v2_formatting() {
        let manager = PromptManager::new().await.unwrap();

        // Test v2 endpoint formatting
        let formatted = manager.format_find_endpoint_v2(
            "send email to john",
            "1. ID: send_email | Description: Send email\n",
            Some("v2"),
        );

        assert!(formatted.contains("send email to john"));
        assert!(formatted.contains("endpoint ID"));

        // Test v2 parameter extraction formatting
        let formatted = manager.format_sentence_to_json_v2(
            "send email to john about meeting",
            "Send an email",
            "to: recipient email",
            "subject: email subject",
            Some("v2"),
        );

        assert!(formatted.contains("send email to john about meeting"));
        assert!(formatted.contains("Send an email"));
        assert!(formatted.contains("to: recipient email"));
    }
}
