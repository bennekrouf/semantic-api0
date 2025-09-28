use regex::Regex;
use serde_json::Value;
use std::error::Error;
use tracing::{debug, error};

pub fn sanitize_json(raw_text: &str) -> Result<Value, Box<dyn Error + Send + Sync>> {
    //debug!("Sanitizing JSON from raw text:\n{}", raw_text);

    // Extract JSON using regex
    let re = Regex::new(r"\{[\s\S]*\}")?;
    let json_str = re
        .find(raw_text)
        .ok_or_else(|| {
            error!("No JSON found in response: {}", raw_text);
            "No JSON structure found in response"
        })?
        .as_str();

    // Remove trailing commas before parsing
    let cleaned_json = remove_trailing_commas(json_str);

    debug!("Cleaned JSON string:\n{}", cleaned_json);

    // Parse the JSON
    let parsed_json: Value = serde_json::from_str(json_str).map_err(|e| {
        error!("Failed to parse JSON: {}\nRaw JSON string: {}", e, json_str);
        format!("Failed to parse JSON: {e}. Raw JSON: {json_str}")
    })?;

    debug!("Successfully parsed JSON");
    Ok(parsed_json)
}

fn remove_trailing_commas(json_str: &str) -> String {
    // First, handle single-line trailing commas
    let single_line_fixed = json_str.replace(", }", "}").replace(",}", "}");

    // Then handle multi-line trailing commas with regex
    let re = Regex::new(r",(\s*[\}\]])").expect("Invalid regex pattern");
    re.replace_all(&single_line_fixed, "$1").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_trailing_commas() {
        let input = r#"{
            "customer_id": "Josiane",
        }"#;
        let expected = r#"{
            "customer_id": "Josiane"
        }"#;
        assert_eq!(remove_trailing_commas(input), expected);
    }

    #[test]
    fn test_sanitize_json_with_trailing_comma() {
        let input = r#"Some text {
            "customer_id": "Josiane",
        } more text"#;
        let result = sanitize_json(input);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["customer_id"], "Josiane");
    }

    #[test]
    fn test_sanitize_valid_json() {
        let input = r#"Some text before {"key": "value"} some text after"#;
        let result = sanitize_json(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["key"], "value");
    }

    #[test]
    fn test_sanitize_invalid_input() {
        let input = "No JSON here";
        let result = sanitize_json(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_complex_json() {
        let input = r#"Here's the output: {
            "nested": {
                "array": [1, 2, 3],
                "object": {"key": "value"}
            }
        } and some text after"#;
        let result = sanitize_json(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["nested"]["array"][0], 1);
    }
}
