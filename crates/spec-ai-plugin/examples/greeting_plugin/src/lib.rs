//! Example custom tool plugin for spec-ai
//!
//! This plugin demonstrates how to create custom tools that can be loaded
//! at runtime by the spec-ai agent.
//!
//! # Building
//!
//! ```bash
//! cargo build --release
//! ```
//!
//! # Installation
//!
//! Copy the built library to your plugins directory:
//!
//! ```bash
//! # macOS
//! cp target/release/libgreeting_plugin.dylib ~/.spec-ai/tools/
//!
//! # Linux
//! cp target/release/libgreeting_plugin.so ~/.spec-ai/tools/
//!
//! # Windows
//! cp target/release/greeting_plugin.dll ~/.spec-ai/tools/
//! ```
//!
//! # Configuration
//!
//! Enable plugins in your spec-ai.config.toml:
//!
//! ```toml
//! [plugins]
//! enabled = true
//! custom_tools_dir = "~/.spec-ai/tools"
//! ```

use abi_stable::{
    library::RootModule,
    sabi_types::VersionStrings,
    std_types::{ROption, RStr, RString, RVec},
    StableAbi,
};
use serde::Deserialize;

// ============================================================================
// ABI Types (must match spec-ai-plugin)
// ============================================================================

/// Plugin API version - must match the host
const PLUGIN_API_VERSION: u32 = 1;

#[repr(C)]
#[derive(StableAbi, Debug, Clone)]
struct PluginToolResult {
    success: bool,
    output: RString,
    error: ROption<RString>,
}

impl PluginToolResult {
    fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: RString::from(output.into()),
            error: ROption::RNone,
        }
    }

    fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: RString::new(),
            error: ROption::RSome(RString::from(error.into())),
        }
    }
}

#[repr(C)]
#[derive(StableAbi, Debug, Clone)]
struct PluginToolInfo {
    name: RString,
    description: RString,
    parameters_json: RString,
}

impl PluginToolInfo {
    fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters_json: impl Into<String>,
    ) -> Self {
        Self {
            name: RString::from(name.into()),
            description: RString::from(description.into()),
            parameters_json: RString::from(parameters_json.into()),
        }
    }
}

#[repr(C)]
#[derive(StableAbi)]
struct PluginTool {
    info: extern "C" fn() -> PluginToolInfo,
    execute: extern "C" fn(args_json: RStr<'_>) -> PluginToolResult,
    initialize: Option<extern "C" fn(context_json: RStr<'_>) -> bool>,
}

type PluginToolRef = &'static PluginTool;

#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = PluginModuleRef)))]
struct PluginModule {
    api_version: extern "C" fn() -> u32,
    get_tools: extern "C" fn() -> RVec<PluginToolRef>,
    plugin_name: extern "C" fn() -> RString,
    #[sabi(last_prefix_field)]
    shutdown: Option<extern "C" fn()>,
}

impl RootModule for PluginModuleRef {
    abi_stable::declare_root_module_statics! {PluginModuleRef}

    const BASE_NAME: &'static str = "spec_ai_plugin";
    const NAME: &'static str = "spec_ai_plugin";
    const VERSION_STRINGS: VersionStrings = abi_stable::package_version_strings!();
}

// ============================================================================
// Greeting Tool Implementation
// ============================================================================

/// Arguments for the greeting tool
#[derive(Debug, Deserialize)]
struct GreetingArgs {
    /// Name of the person to greet
    name: String,
    /// Optional language for the greeting
    #[serde(default)]
    language: Option<String>,
    /// Optional greeting style
    #[serde(default)]
    style: Option<String>,
}

/// Get tool metadata
extern "C" fn greeting_info() -> PluginToolInfo {
    PluginToolInfo::new(
        "greeting",
        "Generates a personalized greeting in various languages and styles",
        r#"{
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the person to greet"
                },
                "language": {
                    "type": "string",
                    "enum": ["en", "es", "fr", "de", "ja"],
                    "description": "Language for the greeting (default: en)"
                },
                "style": {
                    "type": "string",
                    "enum": ["formal", "casual", "enthusiastic"],
                    "description": "Style of the greeting (default: casual)"
                }
            },
            "required": ["name"]
        }"#,
    )
}

/// Execute the greeting tool
extern "C" fn greeting_execute(args_json: RStr<'_>) -> PluginToolResult {
    // Parse arguments
    let args: GreetingArgs = match serde_json::from_str(args_json.as_str()) {
        Ok(a) => a,
        Err(e) => return PluginToolResult::failure(format!("Invalid arguments: {}", e)),
    };

    let language = args.language.as_deref().unwrap_or("en");
    let style = args.style.as_deref().unwrap_or("casual");

    // Generate greeting based on language and style
    let greeting = match (language, style) {
        // English
        ("en", "formal") => format!("Good day, {}. It is a pleasure to meet you.", args.name),
        ("en", "casual") => format!("Hey {}! How's it going?", args.name),
        ("en", "enthusiastic") => format!("WOW! {} is here! This is AMAZING!", args.name),

        // Spanish
        ("es", "formal") => format!("Buenos días, {}. Es un placer conocerle.", args.name),
        ("es", "casual") => format!("¡Hola {}! ¿Qué tal?", args.name),
        ("es", "enthusiastic") => format!("¡{}! ¡Qué alegría verte!", args.name),

        // French
        ("fr", "formal") => format!("Bonjour, {}. Enchanté de faire votre connaissance.", args.name),
        ("fr", "casual") => format!("Salut {}! Ça va?", args.name),
        ("fr", "enthusiastic") => format!("{}! C'est génial de te voir!", args.name),

        // German
        ("de", "formal") => format!("Guten Tag, {}. Es freut mich, Sie kennenzulernen.", args.name),
        ("de", "casual") => format!("Hey {}! Wie geht's?", args.name),
        ("de", "enthusiastic") => format!("{}! Das ist ja super!", args.name),

        // Japanese
        ("ja", "formal") => format!("{}様、初めまして。お会いできて光栄です。", args.name),
        ("ja", "casual") => format!("やあ、{}！元気？", args.name),
        ("ja", "enthusiastic") => format!("{}さん！すごい！会えて嬉しい！", args.name),

        // Default fallback
        _ => format!("Hello, {}!", args.name),
    };

    PluginToolResult::success(greeting)
}

/// Static tool definition
static GREETING_TOOL: PluginTool = PluginTool {
    info: greeting_info,
    execute: greeting_execute,
    initialize: None,
};

// ============================================================================
// Plugin Module Export
// ============================================================================

extern "C" fn api_version() -> u32 {
    PLUGIN_API_VERSION
}

extern "C" fn plugin_name() -> RString {
    RString::from("greeting-plugin")
}

extern "C" fn get_tools() -> RVec<PluginToolRef> {
    RVec::from(vec![&GREETING_TOOL])
}

/// Export the plugin module
///
/// This is the entry point that spec-ai uses to load the plugin.
#[abi_stable::export_root_module]
fn get_library() -> PluginModuleRef {
    PluginModuleRef::from_prefix(PluginModule {
        api_version,
        plugin_name,
        get_tools,
        shutdown: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting_english_casual() {
        let args = r#"{"name": "Alice"}"#;
        let result = greeting_execute(args.into());
        assert!(result.success);
        assert!(result.output.contains("Alice"));
    }

    #[test]
    fn test_greeting_spanish_formal() {
        let args = r#"{"name": "Carlos", "language": "es", "style": "formal"}"#;
        let result = greeting_execute(args.into());
        assert!(result.success);
        assert!(result.output.contains("Carlos"));
        assert!(result.output.contains("Buenos días"));
    }

    #[test]
    fn test_greeting_invalid_args() {
        let args = r#"{"invalid": "args"}"#;
        let result = greeting_execute(args.into());
        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
