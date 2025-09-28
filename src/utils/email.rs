// Create a new file in src/utils/email.rs

use std::error::Error;

pub fn validate_email(email: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    if email.is_empty() {
        return Err("Email is required and cannot be empty".into());
    }

    // Validate email format using regex
    let email_regex = regex::Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
        .expect("Invalid email regex pattern");

    if !email_regex.is_match(email) {
        return Err(format!("Invalid email format: {email}").into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_emails() {
        assert!(validate_email("user@example.com").is_ok());
        assert!(validate_email("user.name+tag@example.co.uk").is_ok());
        assert!(validate_email("first.last@example.org").is_ok());
    }

    #[test]
    fn test_invalid_emails() {
        assert!(validate_email("").is_err());
        assert!(validate_email("user@").is_err());
        assert!(validate_email("user@domain").is_err());
        assert!(validate_email("@domain.com").is_err());
        assert!(validate_email("user name@domain.com").is_err());
    }
}
