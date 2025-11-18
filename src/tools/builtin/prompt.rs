use crate::tools::{Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::IsTerminal;
use std::time::Duration;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader, Stdin};
use tokio::time::timeout;

/// Tool for interactively prompting the human user for additional input.
pub struct PromptUserTool;

impl PromptUserTool {
    pub fn new() -> Self {
        Self
    }

    fn supports_interactive() -> bool {
        std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
    }

    fn default_required() -> bool {
        true
    }

    async fn prompt_interactively(&self, args: &PromptUserArgs) -> Result<NormalizedResponse> {
        if !Self::supports_interactive() {
            return Err(anyhow!(
                "Interactive prompting is unavailable (stdin/stdout not a TTY). Provide `prefilled_response` instead."
            ));
        }

        self.print_prompt_header(args).await?;
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin);
        match args.input_type {
            PromptInputType::MultilineText => self.collect_multiline(&mut reader, args).await,
            _ => self.collect_single_line(&mut reader, args).await,
        }
    }

    async fn collect_single_line(
        &self,
        reader: &mut BufReader<Stdin>,
        args: &PromptUserArgs,
    ) -> Result<NormalizedResponse> {
        let mut stdout = io::stdout();
        loop {
            stdout.write_all(b"> ").await?;
            stdout.flush().await?;
            let mut buffer = String::new();
            let bytes = self
                .read_line(reader, &mut buffer, args.timeout_seconds)
                .await?;
            if bytes == 0 {
                return Err(anyhow!("User input closed before a value was provided"));
            }
            let trimmed = buffer.trim_end().to_string();

            if trimmed.is_empty() {
                if let Some(res) = self.empty_fallback(args)? {
                    return Ok(res);
                }
                self.write_warning(
                    "A response is required. Please provide a value or configure a default.",
                )
                .await?;
                continue;
            }

            match self.parse_user_input(&trimmed, args) {
                Ok(resp) => return Ok(resp),
                Err(err) => {
                    self.write_warning(&format!("{}", err)).await?;
                }
            }
        }
    }

    async fn collect_multiline(
        &self,
        reader: &mut BufReader<Stdin>,
        args: &PromptUserArgs,
    ) -> Result<NormalizedResponse> {
        let mut stdout = io::stdout();
        stdout
            .write_all(
                b"Enter your response. Finish by typing '/end' on its own line or '/skip' to leave empty.\n",
            )
            .await?;
        stdout.flush().await?;

        loop {
            let mut lines = Vec::new();
            loop {
                stdout.write_all(b"| ").await?;
                stdout.flush().await?;
                let mut buffer = String::new();
                let bytes = self
                    .read_line(reader, &mut buffer, args.timeout_seconds)
                    .await?;
                if bytes == 0 {
                    return Err(anyhow!(
                        "User input closed before multiline response finished"
                    ));
                }
                let trimmed = buffer.trim_end();
                if trimmed.eq_ignore_ascii_case("/end") {
                    break;
                }
                if trimmed.eq_ignore_ascii_case("/skip") {
                    if let Some(res) = self.empty_fallback(args)? {
                        return Ok(res);
                    }
                    self.write_warning(
                        "This prompt is required. Please provide content before finishing.",
                    )
                    .await?;
                    continue;
                }
                lines.push(trimmed.to_string());
            }

            if lines.is_empty() {
                if let Some(res) = self.empty_fallback(args)? {
                    return Ok(res);
                }
                self.write_warning("A response is required. Please try again.")
                    .await?;
                continue;
            }

            let combined = lines.join("\n");
            return self.normalize_text_value(combined, args, false, false);
        }
    }

    async fn read_line(
        &self,
        reader: &mut BufReader<Stdin>,
        buffer: &mut String,
        timeout_secs: Option<u64>,
    ) -> Result<usize> {
        buffer.clear();
        if let Some(secs) = timeout_secs {
            match timeout(Duration::from_secs(secs), reader.read_line(buffer)).await {
                Ok(res) => Ok(res.context("Failed to read user input")?),
                Err(_) => Err(anyhow!(
                    "Timed out waiting for user input after {} seconds",
                    secs
                )),
            }
        } else {
            Ok(reader
                .read_line(buffer)
                .await
                .context("Failed to read user input")?)
        }
    }

    async fn write_warning(&self, message: &str) -> Result<()> {
        let mut stdout = io::stdout();
        stdout
            .write_all(format!("⚠️  {}\n", message).as_bytes())
            .await?;
        stdout.flush().await?;
        Ok(())
    }

    async fn print_prompt_header(&self, args: &PromptUserArgs) -> Result<()> {
        let mut stdout = io::stdout();
        let mut section = String::new();
        section.push_str("\n🔸 User Input Required\n");
        section.push_str(&format!("{}\n", args.prompt.trim()));
        if let Some(instructions) = &args.instructions {
            section.push_str(instructions.trim());
            section.push('\n');
        }
        if let Some(placeholder) = &args.placeholder {
            section.push_str(&format!("Hint: {}\n", placeholder));
        }
        if !args.options.is_empty() {
            section.push_str("Options:\n");
            for (idx, opt) in args.options.iter().enumerate() {
                let label = opt.label.as_deref().unwrap_or("(option)");
                let mut line = format!("  [{}] {}", idx + 1, label);
                if let Some(code) = &opt.short_code {
                    line.push_str(&format!(" (code: {})", code));
                }
                if let Some(desc) = &opt.description {
                    line.push_str(&format!(" — {}", desc));
                }
                if let Some(preview) = value_preview(&opt.value) {
                    line.push_str(&format!(" [value: {}]", preview));
                }
                line.push('\n');
                section.push_str(&line);
            }
            if args.allow_freeform {
                section.push_str("  (Custom values are allowed.)\n");
            }
        }
        if let Some(validation) = &args.validation_hint {
            section.push_str(&format!("Validation: {}\n", validation));
        }

        match args.input_type {
            PromptInputType::MultilineText => {
                section.push_str(
                    "Enter text over multiple lines. Type '/end' on its own line when finished.\n",
                );
            }
            PromptInputType::Boolean => {
                section.push_str("Respond with yes/no, true/false, or y/n.\n");
            }
            PromptInputType::Number => {
                section.push_str("Enter a numeric value.\n");
            }
            PromptInputType::SingleSelect | PromptInputType::MultiSelect => {
                section.push_str(
                    "Choose by number, label, or short code. Separate multiple choices with commas.\n",
                );
            }
            PromptInputType::Json => {
                section.push_str("Provide a JSON value (object, array, string, etc.).\n");
            }
            PromptInputType::Text => {}
        }

        stdout.write_all(section.as_bytes()).await?;
        stdout.flush().await?;
        Ok(())
    }

    fn empty_fallback(&self, args: &PromptUserArgs) -> Result<Option<NormalizedResponse>> {
        if let Some(default_value) = &args.default_value {
            let mut normalized = self.normalize_prefill(default_value.clone(), args)?;
            normalized.used_default = true;
            return Ok(Some(normalized));
        }
        if !args.required {
            return Ok(Some(NormalizedResponse::empty()));
        }
        Ok(None)
    }

    fn parse_user_input(&self, raw: &str, args: &PromptUserArgs) -> Result<NormalizedResponse> {
        match args.input_type {
            PromptInputType::Text => self.normalize_text_value(raw.to_string(), args, false, false),
            PromptInputType::MultilineText => {
                self.normalize_text_value(raw.to_string(), args, false, false)
            }
            PromptInputType::Boolean => {
                let value = parse_bool(raw)
                    .ok_or_else(|| anyhow!("Could not interpret '{}' as yes/no", raw))?;
                Ok(NormalizedResponse::from_bool(value))
            }
            PromptInputType::Number => {
                let value: f64 = raw
                    .parse()
                    .map_err(|_| anyhow!("Could not interpret '{}' as a number", raw))?;
                self.normalize_number_value(value, args, false, false)
            }
            PromptInputType::SingleSelect => self.resolve_single_selection(raw, args, false, false),
            PromptInputType::MultiSelect => self.resolve_multi_selection(raw, args, false, false),
            PromptInputType::Json => {
                let value: Value = serde_json::from_str(raw)
                    .map_err(|err| anyhow!("Invalid JSON input: {}", err))?;
                Ok(NormalizedResponse::from_json(value))
            }
        }
    }

    fn normalize_prefill(&self, value: Value, args: &PromptUserArgs) -> Result<NormalizedResponse> {
        match args.input_type {
            PromptInputType::Text | PromptInputType::MultilineText => {
                let text = value_to_owned_string(&value).ok_or_else(|| {
                    anyhow!("prefilled_response must be a string for text prompts")
                })?;
                self.normalize_text_value(text, args, false, true)
            }
            PromptInputType::Boolean => {
                let as_bool = match value {
                    Value::Bool(b) => Some(b),
                    Value::String(s) => parse_bool(&s),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            if i == 0 {
                                Some(false)
                            } else if i == 1 {
                                Some(true)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
                .ok_or_else(|| anyhow!("prefilled_response must be boolean"))?;
                Ok(NormalizedResponse::from_prefilled_bool(as_bool))
            }
            PromptInputType::Number => {
                let numeric = match &value {
                    Value::Number(num) => num.as_f64(),
                    Value::String(s) => s.parse().ok(),
                    _ => None,
                }
                .ok_or_else(|| anyhow!("prefilled_response must be numeric"))?;
                self.normalize_number_value(numeric, args, false, true)
            }
            PromptInputType::SingleSelect => {
                if value.is_null() && !args.required {
                    return Ok(NormalizedResponse::empty());
                }
                self.match_prefilled_selection(value, args, false, true)
            }
            PromptInputType::MultiSelect => self.match_prefilled_multi(value, args, false, true),
            PromptInputType::Json => Ok(NormalizedResponse::from_prefilled_json(value)),
        }
    }

    fn normalize_text_value(
        &self,
        mut text: String,
        args: &PromptUserArgs,
        used_default: bool,
        used_prefill: bool,
    ) -> Result<NormalizedResponse> {
        let len = text.chars().count();
        if let Some(min) = args.min_length {
            if len < min {
                return Err(anyhow!("Response must be at least {} characters", min));
            }
        }
        if let Some(max) = args.max_length {
            if len > max {
                text.truncate(max);
            }
        }
        Ok(NormalizedResponse::from_string(
            text,
            used_default,
            used_prefill,
        ))
    }

    fn normalize_number_value(
        &self,
        value: f64,
        args: &PromptUserArgs,
        used_default: bool,
        used_prefill: bool,
    ) -> Result<NormalizedResponse> {
        if let Some(min) = args.min_value {
            if value < min {
                return Err(anyhow!("Value must be >= {}", min));
            }
        }
        if let Some(max) = args.max_value {
            if value > max {
                return Err(anyhow!("Value must be <= {}", max));
            }
        }
        if let Some(step) = args.step {
            if step > 0.0 {
                let quotient = value / step;
                let nearest = quotient.round();
                if (quotient - nearest).abs() > 1e-6 {
                    return Err(anyhow!("Value must be a multiple of {}", step));
                }
            }
        }
        Ok(NormalizedResponse::from_number(
            value,
            used_default,
            used_prefill,
        ))
    }

    fn resolve_single_selection(
        &self,
        raw: &str,
        args: &PromptUserArgs,
        used_default: bool,
        used_prefill: bool,
    ) -> Result<NormalizedResponse> {
        if args.options.is_empty() && !args.allow_freeform {
            return Err(anyhow!(
                "No options provided. Set `allow_freeform` to true for free-text answers."
            ));
        }

        if args.options.is_empty() {
            return Ok(NormalizedResponse::from_string(
                raw.to_string(),
                used_default,
                used_prefill,
            ));
        }

        match match_option(raw, &args.options) {
            Some((label, value)) => Ok(NormalizedResponse::from_selection(
                value,
                Some(label),
                used_default,
                used_prefill,
            )),
            None if args.allow_freeform => Ok(NormalizedResponse::from_string(
                raw.to_string(),
                used_default,
                used_prefill,
            )),
            None => Err(anyhow!("'{}' did not match any available options", raw)),
        }
    }

    fn resolve_multi_selection(
        &self,
        raw: &str,
        args: &PromptUserArgs,
        used_default: bool,
        used_prefill: bool,
    ) -> Result<NormalizedResponse> {
        if args.options.is_empty() && !args.allow_freeform {
            return Err(anyhow!(
                "Multi-select prompts require options unless `allow_freeform` is true"
            ));
        }
        let tokens: Vec<_> = raw
            .split(',')
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() {
            return Err(anyhow!("Provide at least one selection"));
        }

        let mut values = Vec::new();
        let mut labels = Vec::new();
        for token in tokens {
            if let Some((label, value)) = match_option(token, &args.options) {
                values.push(value);
                labels.push(label);
            } else if args.allow_freeform {
                values.push(Value::String(token.to_string()));
            } else {
                return Err(anyhow!("'{}' did not match any available options", token));
            }
        }
        Ok(NormalizedResponse::from_multi_selection(
            values,
            labels,
            used_default,
            used_prefill,
        ))
    }

    fn match_prefilled_selection(
        &self,
        value: Value,
        args: &PromptUserArgs,
        used_default: bool,
        used_prefill: bool,
    ) -> Result<NormalizedResponse> {
        if args.options.is_empty() {
            if args.allow_freeform {
                return Ok(NormalizedResponse::from_json(value));
            }
            return Err(anyhow!(
                "prefilled_response must correspond to a provided option"
            ));
        }

        for opt in &args.options {
            if opt.value == value {
                let label = opt
                    .label
                    .clone()
                    .or_else(|| value_to_owned_string(&opt.value));
                return Ok(NormalizedResponse::from_selection(
                    opt.value.clone(),
                    label,
                    used_default,
                    used_prefill,
                ));
            }
        }

        if args.allow_freeform {
            return Ok(NormalizedResponse::from_json(value));
        }

        Err(anyhow!("prefilled_response value did not match any option"))
    }

    fn match_prefilled_multi(
        &self,
        value: Value,
        args: &PromptUserArgs,
        used_default: bool,
        used_prefill: bool,
    ) -> Result<NormalizedResponse> {
        let values = if let Value::Array(arr) = value {
            arr
        } else if let Some(s) = value_to_owned_string(&value) {
            s.split(',')
                .map(|t| Value::String(t.trim().to_string()))
                .collect()
        } else {
            return Err(anyhow!(
                "prefilled_response must be an array or comma-delimited string"
            ));
        };

        if values.is_empty() {
            return Err(anyhow!(
                "prefilled_response must contain at least one entry"
            ));
        }

        let mut resolved_values = Vec::new();
        let mut labels = Vec::new();
        for val in values {
            if let Some(opt_label) =
                args.options
                    .iter()
                    .find(|opt| opt.value == val)
                    .and_then(|opt| {
                        opt.label
                            .clone()
                            .or_else(|| value_to_owned_string(&opt.value))
                    })
            {
                resolved_values.push(val.clone());
                labels.push(opt_label);
            } else if args.allow_freeform {
                resolved_values.push(val.clone());
            } else {
                return Err(anyhow!(
                    "prefilled_response contained a value not present in options"
                ));
            }
        }

        Ok(NormalizedResponse::from_multi_selection(
            resolved_values,
            labels,
            used_default,
            used_prefill,
        ))
    }
}

impl Default for PromptUserTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PromptUserTool {
    fn name(&self) -> &str {
        "prompt_user"
    }

    fn description(&self) -> &str {
        "Prompts the human user for structured input (text, boolean, number, selections, or JSON)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {"type": "string", "description": "Friendly prompt shown to the user."},
                "input_type": {
                    "type": "string",
                    "description": "Type of input expected from the user.",
                    "enum": [
                        "text",
                        "multiline_text",
                        "boolean",
                        "number",
                        "single_select",
                        "multi_select",
                        "json"
                    ],
                    "default": "text"
                },
                "placeholder": {"type": "string"},
                "instructions": {"type": "string", "description": "Extra instructions displayed before collecting input."},
                "required": {"type": "boolean", "default": true},
                "options": {
                    "type": "array",
                    "description": "List of allowed options for select-style prompts.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": {"type": "string"},
                            "description": {"type": "string"},
                            "short_code": {"type": "string", "description": "Short alias like 'a', 'b', or 'high'."},
                            "value": {"description": "JSON value returned when this option is chosen."}
                        },
                        "required": ["value"]
                    }
                },
                "allow_freeform": {
                    "type": "boolean",
                    "description": "Allow responses outside the provided options for selection prompts.",
                    "default": false
                },
                "default_value": {
                    "description": "Default value used when the user skips the prompt."
                },
                "prefilled_response": {
                    "description": "Provide a value here to bypass interactive prompting (useful for automated flows)."
                },
                "min_length": {"type": "integer", "minimum": 0},
                "max_length": {"type": "integer", "minimum": 1},
                "min_value": {"type": "number"},
                "max_value": {"type": "number"},
                "step": {
                    "type": "number",
                    "description": "Restrict numeric responses to multiples of this value."
                },
                "validation_hint": {"type": "string", "description": "Text displayed to the user describing validation requirements."},
                "metadata": {"type": "object", "description": "Arbitrary metadata echoed back with the response."},
                "timeout_seconds": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Abort prompting if no input is received within this many seconds."
                }
            },
            "required": ["prompt", "input_type"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let params: PromptUserArgs =
            serde_json::from_value(args).context("Failed to parse prompt_user arguments")?;

        let response = if let Some(prefill) = &params.prefilled_response {
            match self.normalize_prefill(prefill.clone(), &params) {
                Ok(mut resp) => {
                    resp.used_prefill = true;
                    resp
                }
                Err(err) => return Ok(ToolResult::failure(err.to_string())),
            }
        } else {
            match self.prompt_interactively(&params).await {
                Ok(resp) => resp,
                Err(err) => return Ok(ToolResult::failure(err.to_string())),
            }
        };

        let payload = PromptUserPayload {
            prompt: params.prompt,
            input_type: params.input_type.as_str().to_string(),
            response: response.value,
            display_value: response.display_value,
            selections: response.selection_labels,
            metadata: params.metadata,
            used_default: response.used_default,
            used_prefill: response.used_prefill,
        };

        let output = serde_json::to_string(&payload)?;
        Ok(ToolResult::success(output))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PromptInputType {
    Text,
    MultilineText,
    Boolean,
    Number,
    SingleSelect,
    MultiSelect,
    Json,
}

impl PromptInputType {
    fn as_str(&self) -> &'static str {
        match self {
            PromptInputType::Text => "text",
            PromptInputType::MultilineText => "multiline_text",
            PromptInputType::Boolean => "boolean",
            PromptInputType::Number => "number",
            PromptInputType::SingleSelect => "single_select",
            PromptInputType::MultiSelect => "multi_select",
            PromptInputType::Json => "json",
        }
    }
}

impl Default for PromptInputType {
    fn default() -> Self {
        PromptInputType::Text
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PromptOption {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    short_code: Option<String>,
    value: Value,
}

#[derive(Debug, Deserialize)]
struct PromptUserArgs {
    prompt: String,
    #[serde(default)]
    input_type: PromptInputType,
    #[serde(default)]
    placeholder: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default = "PromptUserTool::default_required")]
    required: bool,
    #[serde(default)]
    options: Vec<PromptOption>,
    #[serde(default)]
    allow_freeform: bool,
    #[serde(default)]
    default_value: Option<Value>,
    #[serde(default)]
    prefilled_response: Option<Value>,
    #[serde(default)]
    min_length: Option<usize>,
    #[serde(default)]
    max_length: Option<usize>,
    #[serde(default)]
    min_value: Option<f64>,
    #[serde(default)]
    max_value: Option<f64>,
    #[serde(default)]
    step: Option<f64>,
    #[serde(default)]
    validation_hint: Option<String>,
    #[serde(default)]
    metadata: Option<Value>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
struct PromptUserPayload {
    prompt: String,
    input_type: String,
    response: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selections: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<Value>,
    used_default: bool,
    used_prefill: bool,
}

#[derive(Debug)]
struct NormalizedResponse {
    value: Value,
    display_value: Option<String>,
    selection_labels: Option<Vec<String>>,
    used_default: bool,
    used_prefill: bool,
}

impl NormalizedResponse {
    fn empty() -> Self {
        Self {
            value: Value::Null,
            display_value: None,
            selection_labels: None,
            used_default: false,
            used_prefill: false,
        }
    }

    fn from_string(value: String, used_default: bool, used_prefill: bool) -> Self {
        Self {
            display_value: Some(value.clone()),
            value: Value::String(value),
            selection_labels: None,
            used_default,
            used_prefill,
        }
    }

    fn from_bool(value: bool) -> Self {
        Self {
            display_value: Some(value.to_string()),
            value: Value::Bool(value),
            selection_labels: None,
            used_default: false,
            used_prefill: false,
        }
    }

    fn from_prefilled_bool(value: bool) -> Self {
        Self {
            display_value: Some(value.to_string()),
            value: Value::Bool(value),
            selection_labels: None,
            used_default: false,
            used_prefill: true,
        }
    }

    fn from_number(value: f64, used_default: bool, used_prefill: bool) -> Self {
        Self {
            display_value: Some(value.to_string()),
            value: json!(value),
            selection_labels: None,
            used_default,
            used_prefill,
        }
    }

    fn from_json(value: Value) -> Self {
        Self {
            selection_labels: None,
            display_value: value_to_owned_string(&value),
            value,
            used_default: false,
            used_prefill: false,
        }
    }

    fn from_prefilled_json(value: Value) -> Self {
        Self {
            selection_labels: None,
            display_value: value_to_owned_string(&value),
            value,
            used_default: false,
            used_prefill: true,
        }
    }

    fn from_selection(
        value: Value,
        label: Option<String>,
        used_default: bool,
        used_prefill: bool,
    ) -> Self {
        Self {
            selection_labels: label.clone().map(|l| vec![l.clone()]),
            display_value: label,
            value,
            used_default,
            used_prefill,
        }
    }

    fn from_multi_selection(
        values: Vec<Value>,
        labels: Vec<String>,
        used_default: bool,
        used_prefill: bool,
    ) -> Self {
        Self {
            selection_labels: if labels.is_empty() {
                None
            } else {
                Some(labels)
            },
            display_value: None,
            value: Value::Array(values),
            used_default,
            used_prefill,
        }
    }
}

fn parse_bool(input: &str) -> Option<bool> {
    match input.trim().to_lowercase().as_str() {
        "y" | "yes" | "true" | "t" | "1" => Some(true),
        "n" | "no" | "false" | "f" | "0" => Some(false),
        _ => None,
    }
}

fn match_option(token: &str, options: &[PromptOption]) -> Option<(String, Value)> {
    if options.is_empty() {
        return None;
    }
    let trimmed = token.trim();
    let lower = trimmed.to_lowercase();
    let numeric_choice = trimmed.parse::<usize>().ok();

    for (idx, opt) in options.iter().enumerate() {
        if let Some(choice) = numeric_choice {
            if choice == idx + 1 {
                let label = opt
                    .label
                    .clone()
                    .or_else(|| value_to_owned_string(&opt.value))
                    .unwrap_or_else(|| format!("Option {}", choice));
                return Some((label, opt.value.clone()));
            }
        }

        if let Some(label) = &opt.label {
            if label.to_lowercase() == lower {
                return Some((label.clone(), opt.value.clone()));
            }
        }

        if let Some(code) = &opt.short_code {
            if code.to_lowercase() == lower {
                let label = opt.label.clone().unwrap_or_else(|| code.clone());
                return Some((label, opt.value.clone()));
            }
        }

        if let Some(value_repr) = value_to_owned_string(&opt.value) {
            if value_repr.to_lowercase() == lower {
                let label = opt.label.clone().unwrap_or(value_repr.clone());
                return Some((label, opt.value.clone()));
            }
        }
    }

    None
}

fn value_to_owned_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => Some(s.clone()),
        Value::Array(_) | Value::Object(_) => Some(value.to_string()),
    }
}

fn value_preview(value: &Value) -> Option<String> {
    let repr = value_to_owned_string(value)?;
    if repr.len() > 48 {
        Some(format!("{}…", &repr[..45]))
    } else {
        Some(repr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_prompt_user_prefilled_text() {
        let tool = PromptUserTool::new();
        let args = json!({
            "prompt": "Provide a status update",
            "input_type": "text",
            "prefilled_response": "All systems nominal"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);

        let payload: Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(payload["response"], "All systems nominal");
        assert_eq!(payload["used_prefill"], true);
    }

    #[tokio::test]
    async fn test_prompt_user_prefilled_select() {
        let tool = PromptUserTool::new();
        let args = json!({
            "prompt": "Choose environment",
            "input_type": "single_select",
            "options": [
                {"label": "Production", "short_code": "prod", "value": "prod"},
                {"label": "Staging", "short_code": "stage", "value": "stage"}
            ],
            "prefilled_response": "stage"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success, "error: {:?}", result.error);
        let payload: Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(payload["response"], "stage");
        assert_eq!(payload["selections"].as_array().unwrap()[0], "Staging");
    }

    #[tokio::test]
    async fn test_prompt_user_prefilled_multi_select() {
        let tool = PromptUserTool::new();
        let args = json!({
            "prompt": "Select tags",
            "input_type": "multi_select",
            "options": [
                {"label": "Urgent", "short_code": "u", "value": "urgent"},
                {"label": "Follow-up", "short_code": "f", "value": "follow"}
            ],
            "prefilled_response": ["urgent", "follow"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let payload: Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(payload["response"].as_array().unwrap().len(), 2);
        assert_eq!(payload["used_prefill"], true);
    }

    #[tokio::test]
    async fn test_prompt_user_missing_prefill_fails_when_noninteractive() {
        // Skip this test if running in an interactive terminal
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            eprintln!("Skipping test: running in interactive terminal");
            return;
        }

        let tool = PromptUserTool::new();
        let args = json!({
            "prompt": "Need manual input",
            "input_type": "text"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
        assert!(result
            .error
            .unwrap_or_default()
            .contains("Interactive prompting is unavailable"));
    }

    #[tokio::test]
    async fn test_prompt_user_invalid_prefill_option() {
        let tool = PromptUserTool::new();
        let args = json!({
            "prompt": "Pick a lane",
            "input_type": "single_select",
            "options": [
                {"label": "Blue", "value": "blue"}
            ],
            "prefilled_response": "red"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
    }
}
