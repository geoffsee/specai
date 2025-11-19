//! Test prompt_user tool to see exact output format

use anyhow::Result;
use serde_json::json;
use spec_ai::tools::ToolRegistry;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Testing prompt_user Tool ===\n");

    let registry = ToolRegistry::with_builtin_tools(None, None);
    let prompt_tool = registry
        .get("prompt_user")
        .expect("prompt_user should exist");

    // Test with boolean input and prefilled response
    println!("Test: Boolean input with prefilled 'yes' response");
    let args = json!({
        "prompt": "Do you approve?",
        "input_type": "boolean",
        "required": true,
        "prefilled_response": true,  // Simulate user typing "yes"
    });

    match prompt_tool.execute(args).await {
        Ok(result) if result.success => {
            println!("✓ Success!");
            println!("Raw output:\n{}\n", result.output);

            // Try to parse it
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result.output) {
                println!("Parsed JSON:");
                println!("{}\n", serde_json::to_string_pretty(&parsed)?);

                println!("Value field: {:?}", parsed["value"]);
                println!("Value as bool: {:?}", parsed["value"].as_bool());
            }
        }
        Ok(result) => {
            println!("✗ Failed: {:?}", result.error);
        }
        Err(e) => {
            println!("✗ Error: {}", e);
        }
    }

    Ok(())
}
