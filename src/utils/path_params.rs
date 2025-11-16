use crate::models::{EndpointParameter, EnhancedEndpoint, ParameterMatch};
use std::collections::HashMap;
use std::error::Error;

pub fn add_path_parameters_to_list(
    endpoint: &EnhancedEndpoint,
    mut parameters: Vec<ParameterMatch>,
) -> Result<(Vec<ParameterMatch>, Vec<EndpointParameter>), Box<dyn Error + Send + Sync>> {
    let mut all_endpoint_parameters = endpoint.parameters.clone();

    if let Ok(Some(path_params)) = extract_path_params_from_path(&endpoint.path) {
        for (param_name, _) in path_params {
            if !all_endpoint_parameters.iter().any(|p| p.name == param_name) {
                all_endpoint_parameters.push(EndpointParameter {
                    name: param_name.clone(),
                    description: format!("URL path parameter: {}", param_name),
                    semantic_value: None,
                    alternatives: None,
                    required: Some(true),
                });
            }

            if !parameters.iter().any(|p| p.name == param_name) {
                parameters.push(ParameterMatch {
                    name: param_name.clone(),
                    description: format!("URL path parameter: {}", param_name),
                    value: None,
                });
            }
        }
    }

    Ok((parameters, all_endpoint_parameters))
}

pub fn extract_path_params_from_path(
    path: &str,
) -> Result<Option<HashMap<String, String>>, Box<dyn Error + Send + Sync>> {
    let mut params = HashMap::new();
    let re = regex::Regex::new(r"\{([^}]+)\}").unwrap();

    for cap in re.captures_iter(path) {
        if let Some(param_name) = cap.get(1) {
            params.insert(
                param_name.as_str().to_string(),
                "{{extracted_from_path}}".to_string(),
            );
        }
    }

    if params.is_empty() {
        Ok(None)
    } else {
        Ok(Some(params))
    }
}
