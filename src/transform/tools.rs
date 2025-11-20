use crate::error::{ProxyError, Result};
use crate::metrics::TOOL_METRICS;
use crate::models::claude::{ClaudeTool, ContentBlock};
use crate::models::gemini::{
    FunctionCall, FunctionResponse, GeminiFunctionDeclaration, GeminiTool,
};
use std::time::Instant;

/// Transform Claude tools to Gemini function declarations
///
/// The transformation is straightforward since both use JSON Schema.
/// Main difference: Claude uses `input_schema`, Gemini uses `parameters`.
pub fn transform_tools(claude_tools: Vec<ClaudeTool>) -> Result<Vec<GeminiTool>> {
    let start = Instant::now();

    let function_declarations = claude_tools
        .into_iter()
        .map(transform_tool)
        .collect::<Result<Vec<_>>>()?;

    TOOL_METRICS.record_transformation(start.elapsed());

    Ok(vec![GeminiTool {
        function_declarations,
    }])
}

/// Transform single Claude tool to Gemini function declaration
///
/// Uses cache for performance optimization
fn transform_tool(tool: ClaudeTool) -> Result<GeminiFunctionDeclaration> {
    // Use cache for faster transformation
    crate::cache::TOOL_CACHE.get_or_transform(&tool)
}

/// Transform Gemini function call to Claude tool use block
///
/// Generates a unique tool_use_id and returns the tool use content block.
/// Returns the tool_use_id for state tracking.
pub fn transform_function_call(function_call: &FunctionCall) -> Result<(String, ContentBlock)> {
    // Generate unique tool_use_id
    let id = format!("toolu_{}", uuid::Uuid::new_v4().simple());

    let block = ContentBlock::ToolUse {
        id: id.clone(),
        name: function_call.name.clone(),
        input: function_call.args.clone(),
    };

    Ok((id, block))
}

/// Transform Claude tool result to Gemini function response
///
/// Requires the function name which should be looked up from state.
pub fn transform_tool_result(
    tool_result: &ContentBlock,
    function_name: String,
) -> Result<crate::models::gemini::GeminiPart> {
    use crate::models::gemini::GeminiPart;

    TOOL_METRICS.record_tool_result();

    match tool_result {
        ContentBlock::ToolResult {
            content, is_error, ..
        } => Ok(GeminiPart::FunctionResponse {
            function_response: FunctionResponse {
                name: function_name,
                response: serde_json::json!({
                    "result": content,
                    "error": is_error.unwrap_or(false)
                }),
            },
        }),
        _ => {
            TOOL_METRICS.record_failure();
            Err(ProxyError::TransformationError(
                "Expected tool_result block".into(),
            ))
        }
    }
}

/// Check if a Gemini response contains function calls
pub fn has_function_calls(chunk: &crate::models::gemini::GeminiStreamChunk) -> bool {
    use crate::models::gemini::GeminiPart;

    chunk.candidates.iter().any(|c| {
        c.content.as_ref().is_some_and(|content| {
            content
                .parts
                .iter()
                .any(|part| matches!(part, GeminiPart::FunctionCall { .. }))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::claude::JsonSchema;
    use std::collections::HashMap;

    fn make_test_tool() -> ClaudeTool {
        let mut properties = HashMap::new();
        properties.insert(
            "location".to_string(),
            Box::new(JsonSchema {
                schema_type: "string".to_string(),
                description: Some("City name".to_string()),
                ..Default::default()
            }),
        );

        ClaudeTool {
            name: "get_weather".to_string(),
            description: "Get weather for a location".to_string(),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: Some(vec!["location".to_string()]),
                ..Default::default()
            },
        }
    }

    #[test]
    fn test_transform_single_tool() {
        let tool = make_test_tool();
        let result = transform_tool(tool).unwrap();

        assert_eq!(result.name, "get_weather");
        assert_eq!(result.description, "Get weather for a location");
        assert_eq!(result.parameters.schema_type, "object");
        assert!(result.parameters.properties.is_some());
    }

    #[test]
    fn test_transform_tools_list() {
        let tools = vec![make_test_tool()];
        let result = transform_tools(tools).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].function_declarations.len(), 1);
        assert_eq!(result[0].function_declarations[0].name, "get_weather");
    }

    #[test]
    fn test_transform_function_call() {
        let fc = FunctionCall {
            name: "get_weather".to_string(),
            args: serde_json::json!({
                "location": "San Francisco"
            }),
        };

        let (id, block) = transform_function_call(&fc).unwrap();

        assert!(id.starts_with("toolu_"));

        match block {
            ContentBlock::ToolUse { id: _, name, input } => {
                assert_eq!(name, "get_weather");
                assert_eq!(input["location"], "San Francisco");
            }
            _ => panic!("Expected ToolUse block"),
        }
    }

    #[test]
    fn test_transform_tool_result() {
        use crate::models::gemini::GeminiPart;

        let result_block = ContentBlock::ToolResult {
            tool_use_id: "toolu_123".to_string(),
            content: "Sunny, 72°F".to_string(),
            is_error: None,
        };

        let gemini_part = transform_tool_result(&result_block, "get_weather".to_string()).unwrap();

        match gemini_part {
            GeminiPart::FunctionResponse { function_response } => {
                assert_eq!(function_response.name, "get_weather");
                assert_eq!(function_response.response["result"], "Sunny, 72°F");
                assert_eq!(function_response.response["error"], false);
            }
            _ => panic!("Expected FunctionResponse"),
        }
    }

    #[test]
    fn test_transform_tool_result_with_error() {
        use crate::models::gemini::GeminiPart;

        let result_block = ContentBlock::ToolResult {
            tool_use_id: "toolu_123".to_string(),
            content: "API key invalid".to_string(),
            is_error: Some(true),
        };

        let gemini_part = transform_tool_result(&result_block, "get_weather".to_string()).unwrap();

        match gemini_part {
            GeminiPart::FunctionResponse { function_response } => {
                assert_eq!(function_response.response["error"], true);
            }
            _ => panic!("Expected FunctionResponse"),
        }
    }

    #[test]
    fn test_complex_schema_transformation() {
        // Test nested objects and arrays
        let mut inner_props = HashMap::new();
        inner_props.insert(
            "lat".to_string(),
            Box::new(JsonSchema {
                schema_type: "number".to_string(),
                ..Default::default()
            }),
        );
        inner_props.insert(
            "lon".to_string(),
            Box::new(JsonSchema {
                schema_type: "number".to_string(),
                ..Default::default()
            }),
        );

        let mut properties = HashMap::new();
        properties.insert(
            "coordinates".to_string(),
            Box::new(JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(inner_props),
                ..Default::default()
            }),
        );

        let tool = ClaudeTool {
            name: "complex_tool".to_string(),
            description: "Tool with complex schema".to_string(),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                ..Default::default()
            },
        };

        let result = transform_tool(tool).unwrap();
        assert_eq!(result.name, "complex_tool");

        // Verify nested structure preserved
        let coords_schema = result
            .parameters
            .properties
            .as_ref()
            .unwrap()
            .get("coordinates")
            .unwrap();
        assert_eq!(coords_schema.schema_type, "object");
    }

    #[test]
    fn test_enum_schema_transformation() {
        let mut properties = HashMap::new();
        properties.insert(
            "units".to_string(),
            Box::new(JsonSchema {
                schema_type: "string".to_string(),
                enum_values: Some(vec![
                    serde_json::json!("celsius"),
                    serde_json::json!("fahrenheit"),
                ]),
                ..Default::default()
            }),
        );

        let tool = ClaudeTool {
            name: "weather_with_units".to_string(),
            description: "Get weather with unit choice".to_string(),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                ..Default::default()
            },
        };

        let result = transform_tool(tool).unwrap();
        let units_schema = result
            .parameters
            .properties
            .as_ref()
            .unwrap()
            .get("units")
            .unwrap();

        assert!(units_schema.enum_values.is_some());
        assert_eq!(units_schema.enum_values.as_ref().unwrap().len(), 2);
    }
}
