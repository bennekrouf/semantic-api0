pub mod endpoint {
    tonic::include_proto!("endpoint");
}
use crate::models::config::load_endpoint_client_config;
use endpoint::endpoint_service_client::EndpointServiceClient;
use endpoint::{Endpoint, GetApiGroupsRequest};
use std::error::Error;
use tonic::transport::Channel;
use crate::app_log;
/// Get the default API URL from configuration if not provided via CLI
pub async fn get_default_api_url() -> Result<String, Box<dyn Error + Send + Sync>> {
    let endpoint_client_config = load_endpoint_client_config().await?;
    Ok(endpoint_client_config.default_address)
}

// Convert gRPC Endpoint to our internal Endpoint structure
// pub fn convert_remote_endpoints(
//     api_groups: Vec<endpoint::ApiGroup>,
// ) -> Vec<crate::models::Endpoint> {
//     api_groups
//         .into_iter()
//         .flat_map(|group| {
//             group
//                 .endpoints
//                 .into_iter()
//                 .map(move |re| crate::models::Endpoint {
//                     id: re.id,
//                     text: re.text,
//                     description: re.description,
//                     parameters: re
//                         .parameters
//                         .into_iter()
//                         .map(|rp| crate::models::EndpointParameter {
//                             name: rp.name,
//                             description: rp.description,
//                             required: Some(rp.required == "true"),
//                             alternatives: Some(rp.alternatives),
//                             semantic_value: None,
//                         })
//                         .collect(),
//                 })
//         })
//         .collect()
// }

/// Check if the endpoint service is available
pub async fn check_endpoint_service_health(
    addr: &str,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    app_log!(info, "Checking health of endpoint service at {}", addr);

    match Channel::from_shared(addr.to_string()) {
        Ok(channel) => match channel.connect().await {
            Ok(_) => {
                app_log!(info, "Endpoint service is available at {}", addr);
                Ok(true)
            }
            Err(e) => {
                app_log!(warn, "Endpoint service is not available at {}: {}", addr, e);
                Ok(false)
            }
        },
        Err(e) => {
            app_log!(error, "Invalid endpoint service address {}: {}", addr, e);
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
                app_log!(info, "Remote endpoint service is available at {}", url);
                Ok(true)
            }
            _ => Err(format!("Endpoint service is not available at {url}").into()),
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

    app_log!(info, "Fetching API groups for email: {}", email);

    // Make the streaming call
    let response = client.get_api_groups(request).await?;
    let mut stream = response.into_inner();

    let mut api_groups = Vec::new();

    // Collect all API groups from the stream
    while let Some(response) = stream.message().await? {
        app_log!(info, "Received batch of {} API groups", response.api_groups.len());
        api_groups.extend(response.api_groups);
    }

    // Collect all endpoints from all groups
    let all_endpoints: Vec<Endpoint> = api_groups
        .iter()
        .flat_map(|group| group.endpoints.clone())
        .collect();

    app_log!(info, 
        "Successfully fetched {} endpoints from {} API groups",
        all_endpoints.len(),
        api_groups.len()
    );

    if all_endpoints.is_empty() {
        app_log!(error, "Remote service returned 0 endpoints for email: {}", email);
        app_log!(error, "This means either:");
        app_log!(error, "  1. No endpoints are configured for this user account");
        app_log!(error, "  2. The user email is not registered in the system");
        app_log!(error, "  3. The endpoint service has no data available");

        return Err(format!(
            "No endpoints available for user '{email}'. Please verify your email address or contact your administrator."
        ).into());
    }

    Ok(all_endpoints)
}

pub fn convert_remote_endpoints_enhanced(
    api_groups: Vec<endpoint::ApiGroup>,
) -> Vec<crate::models::EnhancedEndpoint> {
    api_groups
        .into_iter()
        .flat_map(|group| {
            group
                .endpoints
                .into_iter()
                .map(move |re| crate::models::EnhancedEndpoint {
                    id: re.id,
                    name: re.text.clone(),
                    text: re.text,
                    description: re.description,
                    verb: re.verb,
                    base: re.base,
                    path: re.path.clone(),
                    essential_path: extract_essential_path(&re.path),
                    api_group_id: group.id.clone(),
                    api_group_name: group.name.clone(),
                    parameters: re
                        .parameters
                        .into_iter()
                        .map(|rp| crate::models::EndpointParameter {
                            name: rp.name,
                            description: rp.description,
                            required: Some(rp.required == "true"),
                            alternatives: Some(rp.alternatives),
                            semantic_value: None,
                        })
                        .collect(),
                })
        })
        .collect()
}

fn extract_essential_path(path: &str) -> String {
    let essential = path
        .split('/')
        .filter(|segment| !segment.starts_with('{') || !segment.ends_with('}'))
        .collect::<Vec<&str>>()
        .join("/");

    if essential.is_empty() {
        "/".to_string()
    } else {
        essential
    }
}

pub async fn get_enhanced_endpoints(
    addr: &str,
    email: &str,
) -> Result<Vec<crate::models::EnhancedEndpoint>, Box<dyn Error + Send + Sync>> {
    let channel = Channel::from_shared(addr.to_string())?
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .connect()
        .await?;

    let mut client = EndpointServiceClient::new(channel);
    let request = tonic::Request::new(GetApiGroupsRequest {
        email: email.to_string(),
    });

    let response = client.get_api_groups(request).await?;
    let mut stream = response.into_inner();
    let mut api_groups = Vec::new();

    while let Some(response) = stream.message().await? {
        api_groups.extend(response.api_groups);
    }

    let enhanced_endpoints = convert_remote_endpoints_enhanced(api_groups);

    if enhanced_endpoints.is_empty() {
        return Err(format!("No endpoints available for user '{email}'").into());
    }

    Ok(enhanced_endpoints)
}
