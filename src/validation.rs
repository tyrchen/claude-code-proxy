use crate::error::{ProxyError, Result};
use crate::models::claude::{ClaudeTool, JsonSchema};

/// Validate tool schema before transformation
///
/// Ensures the schema is compatible with both Claude and Gemini APIs.
pub fn validate_tool_schema(tool: &ClaudeTool) -> Result<()> {
    // Validate tool name
    if tool.name.is_empty() {
        return Err(ProxyError::InvalidClaudeRequest(
            "Tool name cannot be empty".into(),
        ));
    }

    if tool.name.len() > 64 {
        return Err(ProxyError::InvalidClaudeRequest(format!(
            "Tool name too long: {} (max 64 characters)",
            tool.name.len()
        )));
    }

    // Validate description
    if tool.description.is_empty() {
        return Err(ProxyError::InvalidClaudeRequest(
            "Tool description cannot be empty".into(),
        ));
    }

    // Validate schema
    validate_json_schema(&tool.input_schema, 0)?;

    Ok(())
}

/// Recursively validate JSON schema
fn validate_json_schema(schema: &JsonSchema, depth: usize) -> Result<()> {
    const MAX_DEPTH: usize = 10;

    if depth > MAX_DEPTH {
        return Err(ProxyError::InvalidClaudeRequest(format!(
            "Schema nesting too deep (max {})",
            MAX_DEPTH
        )));
    }

    // Validate type field
    let valid_types = [
        "object", "array", "string", "number", "integer", "boolean", "null",
    ];
    if !valid_types.contains(&schema.schema_type.as_str()) {
        return Err(ProxyError::InvalidClaudeRequest(format!(
            "Invalid schema type: {}",
            schema.schema_type
        )));
    }

    // Validate object schemas
    if schema.schema_type == "object"
        && let Some(properties) = &schema.properties
    {
        for (name, prop_schema) in properties {
            if name.is_empty() {
                return Err(ProxyError::InvalidClaudeRequest(
                    "Property name cannot be empty".into(),
                ));
            }
            validate_json_schema(prop_schema, depth + 1)?;
        }
    }

    // Validate array schemas
    if schema.schema_type == "array"
        && let Some(items) = &schema.items
    {
        validate_json_schema(items, depth + 1)?;
    }

    // Validate numeric constraints
    if let (Some(min), Some(max)) = (schema.minimum, schema.maximum)
        && min > max
    {
        return Err(ProxyError::InvalidClaudeRequest(format!(
            "Invalid range: minimum ({}) > maximum ({})",
            min, max
        )));
    }

    Ok(())
}

/// Validate all tools in a request
pub fn validate_tools(tools: &[ClaudeTool]) -> Result<()> {
    if tools.is_empty() {
        return Ok(());
    }

    // Check for duplicate tool names
    let mut names = std::collections::HashSet::new();
    for tool in tools {
        if !names.insert(&tool.name) {
            return Err(ProxyError::InvalidClaudeRequest(format!(
                "Duplicate tool name: {}",
                tool.name
            )));
        }

        validate_tool_schema(tool)?;
    }

    // Check tool count limit (Gemini supports up to 128 tools)
    if tools.len() > 128 {
        return Err(ProxyError::InvalidClaudeRequest(format!(
            "Too many tools: {} (max 128)",
            tools.len()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_valid_tool() -> ClaudeTool {
        let mut properties = HashMap::new();
        properties.insert(
            "text".to_string(),
            Box::new(JsonSchema {
                schema_type: "string".to_string(),
                ..Default::default()
            }),
        );

        ClaudeTool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                ..Default::default()
            },
        }
    }

    #[test]
    fn test_validate_valid_tool() {
        let tool = make_valid_tool();
        assert!(validate_tool_schema(&tool).is_ok());
    }

    #[test]
    fn test_empty_name() {
        let mut tool = make_valid_tool();
        tool.name = "".to_string();

        let result = validate_tool_schema(&tool);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("name cannot be empty")
        );
    }

    #[test]
    fn test_name_too_long() {
        let mut tool = make_valid_tool();
        tool.name = "a".repeat(65);

        let result = validate_tool_schema(&tool);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name too long"));
    }

    #[test]
    fn test_empty_description() {
        let mut tool = make_valid_tool();
        tool.description = "".to_string();

        let result = validate_tool_schema(&tool);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("description cannot be empty")
        );
    }

    #[test]
    fn test_invalid_schema_type() {
        let mut tool = make_valid_tool();
        tool.input_schema.schema_type = "invalid_type".to_string();

        let result = validate_tool_schema(&tool);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid schema type")
        );
    }

    #[test]
    fn test_nested_schema_depth() {
        // Build a 12-level deep schema recursively
        fn build_nested_schema(depth: usize) -> JsonSchema {
            if depth == 0 {
                JsonSchema {
                    schema_type: "string".to_string(),
                    ..Default::default()
                }
            } else {
                let mut props = HashMap::new();
                props.insert(
                    "nested".to_string(),
                    Box::new(build_nested_schema(depth - 1)),
                );

                JsonSchema {
                    schema_type: "object".to_string(),
                    properties: Some(props),
                    ..Default::default()
                }
            }
        }

        let deep_schema = build_nested_schema(12);

        let result = validate_json_schema(&deep_schema, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too deep"));
    }

    #[test]
    fn test_invalid_numeric_range() {
        let schema = JsonSchema {
            schema_type: "number".to_string(),
            minimum: Some(100.0),
            maximum: Some(10.0), // Invalid: min > max
            ..Default::default()
        };

        let result = validate_json_schema(&schema, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid range"));
    }

    #[test]
    fn test_validate_duplicate_tools() {
        let tool1 = make_valid_tool();
        let tool2 = make_valid_tool(); // Same name

        let result = validate_tools(&[tool1, tool2]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate tool"));
    }

    #[test]
    fn test_validate_too_many_tools() {
        let tools: Vec<_> = (0..129)
            .map(|i| {
                let mut tool = make_valid_tool();
                tool.name = format!("tool_{}", i);
                tool
            })
            .collect();

        let result = validate_tools(&tools);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Too many tools"));
    }

    #[test]
    fn test_validate_empty_tools() {
        let result = validate_tools(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_multiple_valid_tools() {
        let mut tool1 = make_valid_tool();
        tool1.name = "tool_1".to_string();

        let mut tool2 = make_valid_tool();
        tool2.name = "tool_2".to_string();

        let result = validate_tools(&[tool1, tool2]);
        assert!(result.is_ok());
    }
}
