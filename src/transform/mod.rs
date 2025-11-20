pub mod request;
pub mod tools;
pub mod validation;

pub use request::*;
pub use tools::*;
pub use validation::*;

/// Maps Claude model identifiers to appropriate Gemini models
pub fn map_model_name(_claude_model: &str) -> &'static str {
    // Currently all Claude models map to Gemini 3 Pro Preview
    "gemini-3-pro-preview"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_mapping() {
        assert_eq!(
            map_model_name("claude-3-5-sonnet-20241022"),
            "gemini-3-pro-preview"
        );
        assert_eq!(
            map_model_name("claude-3-opus-20240229"),
            "gemini-3-pro-preview"
        );
        assert_eq!(
            map_model_name("claude-3-haiku-20240307"),
            "gemini-3-pro-preview"
        );
        assert_eq!(map_model_name("unknown-model"), "gemini-3-pro-preview");
    }

    #[test]
    fn test_model_mapping_fuzzy() {
        assert_eq!(map_model_name("claude-opus"), "gemini-3-pro-preview");
        assert_eq!(map_model_name("claude-sonnet"), "gemini-3-pro-preview");
        assert_eq!(map_model_name("opus-v2"), "gemini-3-pro-preview");
    }
}
