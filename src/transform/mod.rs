pub mod request;
pub mod validation;

pub use request::*;
pub use validation::*;

/// Maps Claude model identifiers to appropriate Gemini models
pub fn map_model_name(claude_model: &str) -> &'static str {
    // Fuzzy matching to handle version suffixes
    if claude_model.contains("opus") {
        "gemini-1.5-pro" // Opus -> Pro (highest capability)
    } else if claude_model.contains("sonnet") {
        "gemini-2.0-flash-exp" // Sonnet -> Flash (balanced)
    } else if claude_model.contains("haiku") {
        "gemini-2.0-flash-exp" // Haiku -> Flash (speed)
    } else {
        "gemini-2.0-flash-exp" // Default fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_mapping() {
        assert_eq!(
            map_model_name("claude-3-5-sonnet-20241022"),
            "gemini-2.0-flash-exp"
        );
        assert_eq!(map_model_name("claude-3-opus-20240229"), "gemini-1.5-pro");
        assert_eq!(
            map_model_name("claude-3-haiku-20240307"),
            "gemini-2.0-flash-exp"
        );
        assert_eq!(map_model_name("unknown-model"), "gemini-2.0-flash-exp");
    }

    #[test]
    fn test_model_mapping_fuzzy() {
        assert_eq!(map_model_name("claude-opus"), "gemini-1.5-pro");
        assert_eq!(map_model_name("claude-sonnet"), "gemini-2.0-flash-exp");
        assert_eq!(map_model_name("opus-v2"), "gemini-1.5-pro");
    }
}
