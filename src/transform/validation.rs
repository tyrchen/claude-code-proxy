use crate::error::{ProxyError, Result};
use crate::models::claude::ClaudeRequest;

/// Validate Claude request before transformation
pub fn validate_claude_request(req: &ClaudeRequest) -> Result<()> {
    // Check message count
    if req.messages.is_empty() {
        return Err(ProxyError::InvalidClaudeRequest(
            "No messages provided".into(),
        ));
    }

    // Validate first message is from user
    if req.messages[0].role != "user" {
        return Err(ProxyError::InvalidClaudeRequest(
            "First message must be from user".into(),
        ));
    }

    // Validate role alternation (relaxed - just check no consecutive assistants)
    let mut prev_role: Option<&str> = None;
    for msg in &req.messages {
        if let Some(prev) = prev_role
            && prev == "assistant"
            && msg.role == "assistant"
        {
            return Err(ProxyError::InvalidClaudeRequest(
                "Cannot have consecutive assistant messages".into(),
            ));
        }
        prev_role = Some(&msg.role);
    }

    // Check token limits
    if let Some(max_tokens) = req.max_tokens
        && (max_tokens == 0 || max_tokens > 1_000_000)
    {
        return Err(ProxyError::InvalidClaudeRequest(format!(
            "Invalid max_tokens: {}. Must be between 1 and 1,000,000",
            max_tokens
        )));
    }

    // Validate temperature
    if let Some(temp) = req.temperature
        && !(0.0..=2.0).contains(&temp)
    {
        return Err(ProxyError::InvalidClaudeRequest(format!(
            "Invalid temperature: {}. Must be between 0.0 and 2.0",
            temp
        )));
    }

    // Validate top_p
    if let Some(top_p) = req.top_p
        && !(0.0..=1.0).contains(&top_p)
    {
        return Err(ProxyError::InvalidClaudeRequest(format!(
            "Invalid top_p: {}. Must be between 0.0 and 1.0",
            top_p
        )));
    }

    // Validate top_k
    if let Some(top_k) = req.top_k
        && top_k == 0
    {
        return Err(ProxyError::InvalidClaudeRequest(
            "Invalid top_k: 0. Must be greater than 0".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::claude::{ClaudeMessage, ContentType};

    fn make_simple_request() -> ClaudeRequest {
        ClaudeRequest {
            model: "claude-3-5-sonnet".to_string(),
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Text("Hello".to_string()),
            }],
            system: None,
            max_tokens: Some(100),
            temperature: None,
            stop_sequences: None,
            stream: true,
            top_p: None,
            top_k: None,
            tools: None,
        }
    }

    #[test]
    fn test_validate_simple_request() {
        let req = make_simple_request();
        assert!(validate_claude_request(&req).is_ok());
    }

    #[test]
    fn test_validate_empty_messages() {
        let mut req = make_simple_request();
        req.messages.clear();

        let result = validate_claude_request(&req);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No messages provided")
        );
    }

    #[test]
    fn test_validate_first_message_not_user() {
        let mut req = make_simple_request();
        req.messages[0].role = "assistant".to_string();

        let result = validate_claude_request(&req);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("First message must be from user")
        );
    }

    #[test]
    fn test_validate_consecutive_assistant_messages() {
        let mut req = make_simple_request();
        req.messages.push(ClaudeMessage {
            role: "assistant".to_string(),
            content: ContentType::Text("Hi".to_string()),
        });
        req.messages.push(ClaudeMessage {
            role: "assistant".to_string(),
            content: ContentType::Text("How are you?".to_string()),
        });

        let result = validate_claude_request(&req);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("consecutive assistant")
        );
    }

    #[test]
    fn test_validate_max_tokens_zero() {
        let mut req = make_simple_request();
        req.max_tokens = Some(0);

        let result = validate_claude_request(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_max_tokens_too_large() {
        let mut req = make_simple_request();
        req.max_tokens = Some(2_000_000);

        let result = validate_claude_request(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_temperature_negative() {
        let mut req = make_simple_request();
        req.temperature = Some(-0.1);

        let result = validate_claude_request(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_temperature_too_high() {
        let mut req = make_simple_request();
        req.temperature = Some(2.5);

        let result = validate_claude_request(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_temperature_valid() {
        let mut req = make_simple_request();
        req.temperature = Some(0.7);

        assert!(validate_claude_request(&req).is_ok());
    }

    #[test]
    fn test_validate_top_p_out_of_range() {
        let mut req = make_simple_request();
        req.top_p = Some(1.5);

        let result = validate_claude_request(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_top_k_zero() {
        let mut req = make_simple_request();
        req.top_k = Some(0);

        let result = validate_claude_request(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_alternating_roles() {
        let mut req = make_simple_request();
        req.messages.push(ClaudeMessage {
            role: "assistant".to_string(),
            content: ContentType::Text("Hi".to_string()),
        });
        req.messages.push(ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("How are you?".to_string()),
        });

        assert!(validate_claude_request(&req).is_ok());
    }
}
