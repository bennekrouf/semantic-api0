pub mod endpoint {
    tonic::include_proto!("endpoint");
}
use crate::models::config::load_endpoint_client_config;
use endpoint::endpoint_service_client::EndpointServiceClient;
use endpoint::{Endpoint, GetApiGroupsRequest};
use std::error::Error;
use tonic::transport::Channel;
use tracing::{error, info, warn};
/// Get the default API URL from configuration if not provided via CLI
pub async fn get_default_api_url() -> Result<String, Box<dyn Error + Send + Sync>> {
    let endpoint_client_config = load_endpoint_client_config().await?;
    Ok(endpoint_client_config.default_address)
}

// Convert gRPC Endpoint to our internal Endpoint structure
pub fn convert_remote_endpoints(
    api_groups: Vec<endpoint::ApiGroup>,
) -> Vec<crate::models::Endpoint> {
    api_groups
        .into_iter()
        .flat_map(|group| {
            group
                .endpoints
                .into_iter()
                .map(move |re| crate::models::Endpoint {
                    id: re.id,
                    text: re.text,
                    description: re.description,
                    parameters: re
                        .parameters
                        .into_iter()
                        .map(|rp| crate::models::EndpointParameter {
                            name: rp.name,
                            description: rp.description,
                            required: Some(rp.required),
                            alternatives: Some(rp.alternatives),
                            semantic_value: None,
                        })
                        .collect(),
                })
        })
        .collect()
}

/// Check if the endpoint service is available
pub async fn check_endpoint_service_health(
    addr: &str,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    info!("Checking health of endpoint service at {}", addr);

    match Channel::from_shared(addr.to_string()) {
        Ok(channel) => match channel.connect().await {
            Ok(_) => {
                info!("Endpoint service is available at {}", addr);
                Ok(true)
            }
            Err(e) => {
                warn!("Endpoint service is not available at {}: {}", addr, e);
                Ok(false)
            }
        },
        Err(e) => {
            error!("Invalid endpoint service address {}: {}", addr, e);
            Err(Box::new(e))
        }
    }
}

/// Check if endpoints are properly configured
pub async fn verify_endpoints_configuration(
    api_url: Option<String>,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    // Only check remote endpoint service
    if let Some(url) = &api_url {
        match check_endpoint_service_health(url).await {
            Ok(true) => {
                info!("Remote endpoint service is available at {}", url);
                Ok(true)
            }
            _ => Err(format!("Endpoint service is not available at {}", url).into()),
        }
    } else {
        Err("No remote endpoint service URL provided".into())
    }
}

// Optional: function to get default endpoints for development
pub async fn get_default_endpoints(
    addr: &str,
    email: &str,
) -> Result<Vec<endpoint::Endpoint>, Box<dyn Error + Send + Sync>> {
    // Create a channel to the server
    let channel = Channel::from_shared(addr.to_string())?
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .connect()
        .await?;

    // Create the gRPC client
    let mut client = EndpointServiceClient::new(channel);

    // Prepare the request
    let request = tonic::Request::new(GetApiGroupsRequest {
        email: email.to_string(),
    });

    info!("Fetching API groups for email: {}", email);

    // Make the streaming call
    let response = client.get_api_groups(request).await?;
    let mut stream = response.into_inner();

    let mut api_groups = Vec::new();

    // Collect all API groups from the stream
    while let Some(response) = stream.message().await? {
        info!("Received batch of {} API groups", response.api_groups.len());
        api_groups.extend(response.api_groups);
    }

    // Collect all endpoints from all groups
    let all_endpoints: Vec<Endpoint> = api_groups
        .iter()
        .flat_map(|group| group.endpoints.clone())
        .collect();

    info!(
        "Successfully fetched {} endpoints from {} API groups",
        all_endpoints.len(),
        api_groups.len()
    );

    if all_endpoints.is_empty() {
        error!("Remote service returned 0 endpoints for email: {}", email);
        error!("This means either:");
        error!("  1. No endpoints are configured for this user account");
        error!("  2. The user email is not registered in the system");
        error!("  3. The endpoint service has no data available");

        return Err(format!(
            "No endpoints available for user '{}'. Please verify your email address or contact your administrator.",
            email
        ).into());
    }

    Ok(all_endpoints)
}
