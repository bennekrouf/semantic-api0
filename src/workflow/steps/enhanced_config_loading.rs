use crate::app_log;
use crate::endpoint_client::{check_endpoint_service_health, get_enhanced_endpoints};
use crate::models::config::load_models_config;
use crate::models::{ConfigFile, Endpoint};
use crate::utils::email::validate_email;
use crate::workflow::{WorkflowContext, WorkflowStep};
use async_trait::async_trait;
use std::error::Error;

pub struct EnhancedConfigurationLoadingStep {
    pub api_url: Option<String>,
    pub email: String,
}

#[async_trait]
impl WorkflowStep for EnhancedConfigurationLoadingStep {
    async fn execute(
        &self,
        context: &mut WorkflowContext,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        app_log!(
            info,
            "Loading enhanced configurations with complete endpoint metadata"
        );

        if self.email.is_empty() {
            return Err("Email is required and cannot be empty".into());
        }

        validate_email(&self.email)?;
        context.email = Some(self.email.clone());

        let api_url = self.api_url.as_ref().ok_or("No API URL provided")?;

        match check_endpoint_service_health(api_url).await {
            Ok(true) => {
                app_log!(
                    info,
                    "Remote endpoint service available, fetching enhanced endpoints"
                );

                match get_enhanced_endpoints(api_url, &self.email).await {
                    Ok(enhanced_endpoints) => {
                        if enhanced_endpoints.is_empty() {
                            return Err(format!(
                                "No endpoints found for user '{}'. Contact administrator.",
                                self.email
                            )
                            .into());
                        }

                        let regular_endpoints: Vec<Endpoint> = enhanced_endpoints
                            .iter()
                            .map(|e| Endpoint {
                                id: e.id.clone(),
                                text: e.text.clone(),
                                description: e.description.clone(),
                                parameters: e.parameters.clone(),
                            })
                            .collect();

                        context.endpoints_config = Some(ConfigFile {
                            endpoints: regular_endpoints,
                        });
                        context.enhanced_endpoints = Some(enhanced_endpoints);

                        app_log!(
                            info,
                            "Successfully loaded {} enhanced endpoints",
                            context.enhanced_endpoints.as_ref().unwrap().len()
                        );
                    }
                    Err(e) => {
                        return Err(format!("Failed to fetch enhanced endpoints: {e}").into());
                    }
                }
            }
            Ok(false) | Err(_) => {
                return Err("Remote endpoint service is unavailable".into());
            }
        }

        let models_config = load_models_config().await?;
        context.models_config = Some(models_config);

        Ok(())
    }

    fn name(&self) -> &'static str {
        "enhanced_configuration_loading"
    }
}
