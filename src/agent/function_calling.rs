//! OpenAI Function Calling Integration
//!
//! This module provides functionality to convert tool definitions into OpenAI's
//! ChatCompletionTool format and parse function call responses from the SDK.

use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
use serde_json::{json, Value};

/// Converts parameters to OpenAI function schema format
fn parameters_to_openai_schema(params: &Value) -> Value {
    // Extract properties and required fields from the input
    let properties = params
        .get("properties")
        .and_then(|p| p.as_object())
        .cloned()
        .unwrap_or_default();

    let required: Vec<String> = params
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    json!({
        "type": "object",
        "properties": properties,
        "required": required
    })
}

/// Converts a tool definition into OpenAI ChatCompletionTool format
pub fn tool_to_openai_function(
    name: &str,
    description: &str,
    parameters: &Value,
) -> ChatCompletionTool {
    let schema = parameters_to_openai_schema(parameters);

    ChatCompletionTool {
        r#type: ChatCompletionToolType::Function,
        function: FunctionObject {
            name: name.to_string(),
            description: Some(description.to_string()),
            parameters: Some(schema),
            strict: Some(false),
        },
    }
}

/// Represents a parsed tool call from OpenAI's function calling response
#[derive(Debug, Clone)]
pub struct FunctionCall {
    /// The name of the function/tool to call
    pub name: String,
    /// The arguments as a JSON value
    pub arguments: Value,
}

/// Parses a tool call from OpenAI's ChatCompletionResponseMessage
/// Expects the tool call to be in the message's tool_calls array
pub fn parse_tool_call_from_message(
    _tool_call_id: &str,
    function_name: &str,
    arguments_str: &str,
) -> Option<(String, Value)> {
    // Parse the JSON arguments string
    let args = serde_json::from_str(arguments_str).ok()?;
    Some((function_name.to_string(), args))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_to_openai_function() {
        let params = json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message to echo"
                }
            },
            "required": ["message"]
        });

        let tool = tool_to_openai_function("echo", "Echo a message", &params);

        assert_eq!(tool.function.name, "echo");
        assert_eq!(
            tool.function.description,
            Some("Echo a message".to_string())
        );
        assert!(tool.function.parameters.is_some());
    }

    #[test]
    fn test_parse_tool_call_from_message() {
        let result = parse_tool_call_from_message("call_123", "echo", r#"{"message": "hello"}"#);

        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "echo");
        assert_eq!(args["message"], "hello");
    }

    #[test]
    fn test_parse_tool_call_invalid_json() {
        let result = parse_tool_call_from_message("call_123", "echo", "invalid json");
        assert!(result.is_none());
    }
}
