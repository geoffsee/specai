use crate::tools::{Tool, ToolResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

/// A calculator tool backed by a small standard math library
pub struct MathTool;

#[derive(Debug, Deserialize)]
struct MathArgs {
    operation: String,
    a: f64,
    b: f64,
}

impl MathTool {
    pub fn new() -> Self {
        Self
    }

    fn evaluate(&self, operation: &str, a: f64, b: f64) -> Result<f64> {
        match operation {
            "add" | "+" => Ok(a + b),
            "subtract" | "-" => Ok(a - b),
            "multiply" | "*" => Ok(a * b),
            "divide" | "/" => {
                if b == 0.0 {
                    anyhow::bail!("Division by zero");
                }
                Ok(a / b)
            }
            "power" | "**" => Ok(a.powf(b)),
            "modulo" | "%" => {
                if b == 0.0 {
                    anyhow::bail!("Modulo by zero");
                }
                Ok(a % b)
            }
            // Unary functions – operate on `a`, ignore `b`
            "sqrt" => {
                if a < 0.0 {
                    anyhow::bail!("Cannot compute square root of a negative number");
                }
                Ok(a.sqrt())
            }
            "abs" => Ok(a.abs()),
            "exp" => Ok(a.exp()),
            "ln" => {
                if a <= 0.0 {
                    anyhow::bail!("Natural logarithm is only defined for positive values");
                }
                Ok(a.ln())
            }
            "log10" => {
                if a <= 0.0 {
                    anyhow::bail!("Base-10 logarithm is only defined for positive values");
                }
                Ok(a.log10())
            }
            "log2" => {
                if a <= 0.0 {
                    anyhow::bail!("Base-2 logarithm is only defined for positive values");
                }
                Ok(a.log2())
            }
            "sin" => Ok(a.sin()),
            "cos" => Ok(a.cos()),
            "tan" => Ok(a.tan()),
            "asin" => {
                if a < -1.0 || a > 1.0 {
                    anyhow::bail!("asin is only defined for inputs between -1 and 1");
                }
                Ok(a.asin())
            }
            "acos" => {
                if a < -1.0 || a > 1.0 {
                    anyhow::bail!("acos is only defined for inputs between -1 and 1");
                }
                Ok(a.acos())
            }
            "atan" => Ok(a.atan()),
            "sinh" => Ok(a.sinh()),
            "cosh" => Ok(a.cosh()),
            "tanh" => Ok(a.tanh()),
            // Binary functions – use both `a` and `b`
            "min" => Ok(a.min(b)),
            "max" => Ok(a.max(b)),
            "hypot" => Ok(a.hypot(b)),
            "atan2" => Ok(a.atan2(b)),
            _ => anyhow::bail!("Unsupported operation: {}", operation),
        }
    }
}

impl Default for MathTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for MathTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Calculator tool: performs mathematical operations using a small standard library (arithmetic, powers, modulo, roots, logs, trigonometric and hyperbolic functions, and simple two-argument operations like min/max)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "description": "The operation to perform. Supports arithmetic (add, subtract, multiply, divide, power, modulo or +, -, *, /, **, %), common unary functions (sqrt, abs, exp, ln, log10, log2, sin, cos, tan, asin, acos, atan, sinh, cosh, tanh), and simple binary functions (min, max, hypot, atan2). For unary functions, only `a` is used.",
                    "enum": [
                        "add", "subtract", "multiply", "divide", "power", "modulo",
                        "+", "-", "*", "/", "**", "%",
                        "sqrt", "abs", "exp", "ln", "log10", "log2",
                        "sin", "cos", "tan", "asin", "acos", "atan",
                        "sinh", "cosh", "tanh",
                        "min", "max", "hypot", "atan2"
                    ]
                },
                "a": {
                    "type": "number",
                    "description": "The first operand (or the sole operand for unary functions)"
                },
                "b": {
                    "type": "number",
                    "description": "The second operand (ignored for unary functions such as sqrt, ln, sin, etc.)"
                }
            },
            "required": ["operation", "a", "b"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let math_args: MathArgs =
            serde_json::from_value(args).context("Failed to parse math arguments")?;

        match self.evaluate(&math_args.operation, math_args.a, math_args.b) {
            Ok(result) => Ok(ToolResult::success(result.to_string())),
            Err(e) => Ok(ToolResult::failure(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_math_tool_basic() {
        let tool = MathTool::new();

        assert_eq!(tool.name(), "calculator");
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn test_math_tool_parameters() {
        let tool = MathTool::new();
        let params = tool.parameters();

        assert!(params.is_object());
        assert!(params["properties"]["operation"].is_object());
        assert!(params["properties"]["a"].is_object());
        assert!(params["properties"]["b"].is_object());
    }

    #[tokio::test]
    async fn test_math_tool_add() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "add",
            "a": 5.0,
            "b": 3.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, "8");
    }

    #[tokio::test]
    async fn test_math_tool_add_symbol() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "+",
            "a": 10.5,
            "b": 2.5
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, "13");
    }

    #[tokio::test]
    async fn test_math_tool_subtract() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "subtract",
            "a": 10.0,
            "b": 3.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, "7");
    }

    #[tokio::test]
    async fn test_math_tool_multiply() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "multiply",
            "a": 4.0,
            "b": 5.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, "20");
    }

    #[tokio::test]
    async fn test_math_tool_divide() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "divide",
            "a": 15.0,
            "b": 3.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, "5");
    }

    #[tokio::test]
    async fn test_math_tool_divide_by_zero() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "divide",
            "a": 10.0,
            "b": 0.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Division by zero"));
    }

    #[tokio::test]
    async fn test_math_tool_power() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "power",
            "a": 2.0,
            "b": 3.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, "8");
    }

    #[tokio::test]
    async fn test_math_tool_modulo() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "modulo",
            "a": 10.0,
            "b": 3.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, "1");
    }

    #[tokio::test]
    async fn test_math_tool_modulo_by_zero() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "modulo",
            "a": 10.0,
            "b": 0.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_math_tool_invalid_operation() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "invalid",
            "a": 10.0,
            "b": 3.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_math_tool_missing_arguments() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "add"
        });

        let result = tool.execute(args).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_math_tool_negative_numbers() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "add",
            "a": -5.0,
            "b": 3.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, "-2");
    }

    #[tokio::test]
    async fn test_math_tool_decimal_numbers() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "multiply",
            "a": 2.5,
            "b": 4.2
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        let output: f64 = result.output.parse().unwrap();
        assert!((output - 10.5).abs() < 0.0001);
    }

    #[tokio::test]
    async fn test_math_tool_sqrt() {
        let tool = MathTool::new();
        let args = serde_json::json!({
            "operation": "sqrt",
            "a": 16.0,
            "b": 0.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        let output: f64 = result.output.parse().unwrap();
        assert!((output - 4.0).abs() < 0.0001);
    }

    #[tokio::test]
    async fn test_math_tool_sin() {
        let tool = MathTool::new();
        // sin(pi/2) ≈ 1.0
        let args = serde_json::json!({
            "operation": "sin",
            "a": std::f64::consts::FRAC_PI_2,
            "b": 0.0
        });

        let result = tool.execute(args).await.unwrap();

        assert!(result.success);
        let output: f64 = result.output.parse().unwrap();
        assert!((output - 1.0).abs() < 0.0001);
    }
}
