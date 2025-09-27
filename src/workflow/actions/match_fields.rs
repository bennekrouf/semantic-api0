// src/workflow/actions/match_fields.rs - Generic industry-agnostic implementation

use crate::json_helper::sanitize_json;
use crate::models::config::load_models_config;
use crate::models::Endpoint;
use crate::prompts::PromptManager;
use serde_json::Value;
use std::error::Error;
use tracing::debug;

use crate::models::providers::ModelProvider;
use std::sync::Arc;

pub async fn match_fields_semantic(
    input_json: &Value,
    endpoint: &Endpoint,
    provider: Arc<dyn ModelProvider>,
) -> Result<Vec<(String, String, Option<String>)>, Box<dyn Error + Send + Sync>> {
    debug!("Starting generic semantic field matching");
    debug!("Input JSON: {}", serde_json::to_string_pretty(input_json)?);
    debug!(
        "Endpoint: {} with {} parameters",
        endpoint.id,
        endpoint.parameters.len()
    );

    // Extract the fields from the LLM's JSON response
    let extracted_fields = extract_fields_from_json(input_json)?;
    if extracted_fields.is_empty() {
        debug!("No fields extracted from input JSON");
        return create_empty_matches(&endpoint.parameters);
    }

    debug!(
        "Extracted fields: {:?}",
        extracted_fields.keys().collect::<Vec<_>>()
    );

    // Try direct matching first (fast path)
    let direct_matches = try_direct_matching(&endpoint.parameters, &extracted_fields);

    // Count how many required parameters still need matching
    let unmatched_required = count_unmatched_required_params(&endpoint.parameters, &direct_matches);

    if unmatched_required == 0 {
        debug!("All required parameters matched directly, skipping semantic matching");
        return Ok(direct_matches);
    }

    debug!(
        "Found {} unmatched required parameters, attempting semantic matching",
        unmatched_required
    );

    // Use LLM for semantic matching
    let semantic_matches = try_semantic_matching(
        &endpoint.parameters,
        &extracted_fields,
        &direct_matches,
        provider,
    )
    .await?;

    Ok(semantic_matches)
}

fn extract_fields_from_json(
    input_json: &Value,
) -> Result<serde_json::Map<String, Value>, Box<dyn Error + Send + Sync>> {
    if let Some(endpoints_array) = input_json.get("endpoints").and_then(|e| e.as_array()) {
        if let Some(first_endpoint) = endpoints_array.first() {
            if let Some(fields) = first_endpoint.get("fields").and_then(|f| f.as_object()) {
                return Ok(fields.clone());
            }
        }
    }

    // Fallback: try to use the JSON directly if it's an object
    if let Some(obj) = input_json.as_object() {
        if !obj.contains_key("endpoints") {
            return Ok(obj.clone());
        }
    }

    Ok(serde_json::Map::new())
}

fn try_direct_matching(
    endpoint_params: &[crate::models::EndpointParameter],
    extracted_fields: &serde_json::Map<String, Value>,
) -> Vec<(String, String, Option<String>)> {
    let mut matches = Vec::new();

    for param in endpoint_params {
        let mut matched_value: Option<String> = None;

        // Try exact parameter name match
        if let Some(value) = extracted_fields.get(&param.name) {
            matched_value = extract_string_value(value);
            debug!("Direct match for '{}': {:?}", param.name, matched_value);
        }

        // Try alternatives if provided and no direct match
        if matched_value.is_none() {
            if let Some(alternatives) = &param.alternatives {
                for alt in alternatives {
                    if let Some(value) = extracted_fields.get(alt) {
                        matched_value = extract_string_value(value);
                        debug!(
                            "Alternative match '{}' -> '{}': {:?}",
                            alt, param.name, matched_value
                        );
                        break;
                    }
                }
            }
        }

        matches.push((param.name.clone(), param.description.clone(), matched_value));
    }

    matches
}

fn count_unmatched_required_params(
    endpoint_params: &[crate::models::EndpointParameter],
    matches: &[(String, String, Option<String>)],
) -> usize {
    endpoint_params
        .iter()
        .filter(|param| param.required.unwrap_or(false))
        .filter(|param| {
            !matches.iter().any(|(name, _, value)| {
                name == &param.name
                    && value
                        .as_ref()
                        .map(|v| !v.trim().is_empty())
                        .unwrap_or(false)
            })
        })
        .count()
}

async fn try_semantic_matching(
    endpoint_params: &[crate::models::EndpointParameter],
    extracted_fields: &serde_json::Map<String, Value>,
    direct_matches: &[(String, String, Option<String>)],
    provider: Arc<dyn ModelProvider>,
) -> Result<Vec<(String, String, Option<String>)>, Box<dyn Error + Send + Sync>> {
    // Prepare input for LLM
    let input_fields_str = serde_json::to_string_pretty(extracted_fields)?;

    let parameters_str = endpoint_params
        .iter()
        .map(|p| {
            let required_str = if p.required.unwrap_or(false) {
                " (REQUIRED)"
            } else {
                " (optional)"
            };
            let alternatives_str = if let Some(alts) = &p.alternatives {
                if !alts.is_empty() {
                    format!(" [alternatives: {}]", alts.join(", "))
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            format!(
                "- {}{}: {}{}",
                p.name, required_str, p.description, alternatives_str
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt_manager = PromptManager::new().await?;
    let prompt = prompt_manager
        .get_prompt("match_fields", Some("v1"))
        .ok_or("match_fields v3 prompt not found")?
        .replace("{input_fields}", &input_fields_str)
        .replace("{parameters}", &parameters_str);

    debug!(
        "Semantic matching prompt generated, length: {} chars",
        prompt.len()
    );

    let models_config = load_models_config().await?;
    let model_config = &models_config.default;

    let result = provider.generate(&prompt, model_config).await?;
    debug!("Semantic matching raw response: {}", result.content);

    // Parse the LLM response
    let semantic_json = sanitize_json(&result.content)?;
    debug!("Parsed semantic matching JSON: {:?}", semantic_json);

    // Combine direct matches with semantic matches
    let mut final_matches = Vec::new();

    for param in endpoint_params {
        let mut final_value: Option<String> = None;

        // First, check if we had a direct match
        if let Some((_, _, direct_value)) = direct_matches
            .iter()
            .find(|(name, _, _)| name == &param.name)
        {
            if direct_value
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            {
                final_value = direct_value.clone();
                debug!("Using direct match for '{}': {:?}", param.name, final_value);
            }
        }

        // If no direct match, try semantic match
        if final_value.is_none() {
            if let Some(semantic_value) = semantic_json.get(&param.name) {
                final_value = extract_string_value(semantic_value);
                debug!(
                    "Using semantic match for '{}': {:?}",
                    param.name, final_value
                );
            }
        }

        final_matches.push((param.name.clone(), param.description.clone(), final_value));
    }

    debug!(
        "Final semantic matches: {:?}",
        final_matches
            .iter()
            .map(|(n, _, v)| (n, v))
            .collect::<Vec<_>>()
    );
    Ok(final_matches)
}

fn create_empty_matches(
    endpoint_params: &[crate::models::EndpointParameter],
) -> Result<Vec<(String, String, Option<String>)>, Box<dyn Error + Send + Sync>> {
    Ok(endpoint_params
        .iter()
        .map(|param| (param.name.clone(), param.description.clone(), None))
        .collect())
}

fn extract_string_value(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Null => None,
        Value::Object(_) | Value::Array(_) => {
            // For complex objects, serialize them as JSON strings
            Some(serde_json::to_string(value).unwrap_or_default())
        }
        _ => {
            let string_val = value.to_string().trim_matches('"').to_string();
            if string_val.trim().is_empty() || string_val == "null" {
                None
            } else {
                Some(string_val)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_string_value() {
        assert_eq!(
            extract_string_value(&Value::String("test".to_string())),
            Some("test".to_string())
        );
        assert_eq!(extract_string_value(&Value::String("".to_string())), None);
        assert_eq!(
            extract_string_value(&Value::String("   ".to_string())),
            None
        );
        assert_eq!(extract_string_value(&Value::Null), None);

        let obj = serde_json::json!({"name": "John"});
        let result = extract_string_value(&obj);
        assert!(result.is_some());
        assert!(result.unwrap().contains("John"));
    }

    #[test]
    fn test_count_unmatched_required_params() {
        let params = vec![
            crate::models::EndpointParameter {
                name: "required1".to_string(),
                description: "".to_string(),
                required: Some(true),
                alternatives: None,
                semantic_value: None,
            },
            crate::models::EndpointParameter {
                name: "optional1".to_string(),
                description: "".to_string(),
                required: Some(false),
                alternatives: None,
                semantic_value: None,
            },
        ];

        let matches = vec![
            (
                "required1".to_string(),
                "".to_string(),
                Some("value".to_string()),
            ),
            ("optional1".to_string(), "".to_string(), None),
        ];

        assert_eq!(count_unmatched_required_params(&params, &matches), 0);
    }
}
