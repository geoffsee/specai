use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Structured spec describing a full agent run.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentSpec {
    /// Optional friendly name for the spec.
    pub name: Option<String>,
    /// Primary objective for the run (required).
    pub goal: String,
    /// Additional background/context for the task.
    pub context: Option<String>,
    /// Ordered tasks the agent should complete.
    #[serde(default)]
    pub tasks: Vec<String>,
    /// Expected outputs for the run.
    #[serde(default)]
    pub deliverables: Vec<String>,
    /// Constraints/guardrails the agent should respect.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Source path for this spec when loaded from disk.
    #[serde(skip)]
    source: Option<PathBuf>,
}

impl AgentSpec {
    /// Load a spec from a `.spec` TOML file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            bail!("spec file '{}' was not found", path.display());
        }
        if !Self::is_spec_extension(path) {
            bail!(
                "spec files must use the `.spec` extension (got '{}')",
                path.display()
            );
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed reading spec file '{}'", path.display()))?;
        let mut spec = Self::from_str(&raw)?;
        spec.source = Some(path.to_path_buf());
        Ok(spec)
    }

    /// Parse a spec from TOML content.
    pub fn from_str(contents: &str) -> Result<Self> {
        let spec: AgentSpec = toml::from_str(contents).context("failed to parse spec TOML")?;
        spec.validate()?;
        Ok(spec)
    }

    /// Convert the structured spec into a model prompt.
    pub fn to_prompt(&self) -> String {
        let mut sections = Vec::new();
        if let Some(name) = &self.name {
            if !name.trim().is_empty() {
                sections.push(format!("Spec Name: {}", name.trim()));
            }
        }
        sections.push(format!("Primary Goal:\n{}", self.goal.trim()));

        if let Some(ctx) = self.context_text() {
            sections.push(format!("Context:\n{}", ctx));
        }

        if let Some(tasks) = self.formatted_list("Tasks", &self.tasks, true) {
            sections.push(tasks);
        }
        if let Some(deliverables) = self.formatted_list("Deliverables", &self.deliverables, true) {
            sections.push(deliverables);
        }
        if let Some(constraints) = self.formatted_list("Constraints", &self.constraints, false) {
            sections.push(constraints);
        }

        let mut prompt = String::from(
            "You have been provided with a structured execution spec from the user.\n\
            Follow every goal, task, and deliverable precisely. Reference section names when responding.\n\n",
        );
        prompt.push_str(&sections.join("\n\n"));
        prompt.push_str(
            "\n\nWhen complete, explicitly explain how each deliverable was satisfied and call out any blockers.",
        );
        prompt
    }

    /// Short textual preview for CLI output.
    pub fn preview(&self) -> String {
        let mut preview = Vec::new();
        if let Some(name) = &self.name {
            if !name.trim().is_empty() {
                preview.push(format!("Name: {}", name.trim()));
            }
        }
        preview.push(format!("Goal: {}", self.goal.trim()));
        if let Some(ctx) = self.context_preview(2) {
            preview.push(format!("Context: {}", ctx));
        }
        if let Some(tasks) = self.preview_list("Tasks", &self.tasks) {
            preview.push(tasks);
        }
        if let Some(deliverables) = self.preview_list("Deliverables", &self.deliverables) {
            preview.push(deliverables);
        }
        if let Some(constraints) = self.preview_list("Constraints", &self.constraints) {
            preview.push(constraints);
        }
        preview.join("\n")
    }

    /// Display-friendly name for this spec.
    pub fn display_name(&self) -> &str {
        if let Some(name) = &self.name {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
        self.goal.trim()
    }

    /// Source path if loaded from disk.
    pub fn source_path(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    fn context_text(&self) -> Option<String> {
        self.context
            .as_ref()
            .map(|ctx| ctx.trim())
            .filter(|ctx| !ctx.is_empty())
            .map(|ctx| ctx.to_string())
    }

    fn formatted_list(&self, label: &str, items: &[String], number_items: bool) -> Option<String> {
        let normalized = Self::normalized_items(items);
        if normalized.is_empty() {
            return None;
        }

        let formatted = normalized
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                if number_items {
                    format!("{}. {}", idx + 1, item)
                } else {
                    format!("- {}", item)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        Some(format!("{}:\n{}", label, formatted))
    }

    fn preview_list(&self, label: &str, items: &[String]) -> Option<String> {
        let normalized = Self::normalized_items(items);
        if normalized.is_empty() {
            return None;
        }

        let mut lines = normalized
            .iter()
            .take(3)
            .enumerate()
            .map(|(idx, item)| format!("  {}. {}", idx + 1, item))
            .collect::<Vec<_>>();

        if normalized.len() > 3 {
            lines.push(format!("  ... ({} more)", normalized.len() - 3));
        }

        Some(format!("{}:\n{}", label, lines.join("\n")))
    }

    fn context_preview(&self, max_lines: usize) -> Option<String> {
        self.context_text().map(|ctx| {
            let lines: Vec<&str> = ctx
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            if lines.is_empty() {
                return ctx;
            }

            lines
                .into_iter()
                .take(max_lines)
                .collect::<Vec<_>>()
                .join(" / ")
        })
    }

    fn normalized_items(items: &[String]) -> Vec<String> {
        items
            .iter()
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .map(|item| item.to_string())
            .collect()
    }

    fn validate(&self) -> Result<()> {
        if self.goal.trim().is_empty() {
            bail!("spec goal must be provided");
        }

        let has_tasks = !Self::normalized_items(&self.tasks).is_empty();
        let has_deliverables = !Self::normalized_items(&self.deliverables).is_empty();
        if !has_tasks && !has_deliverables {
            bail!("spec must include at least one task or deliverable");
        }

        Ok(())
    }

    fn is_spec_extension(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("spec"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_spec_and_generates_prompt() {
        let contents = r#"
name = "Docs refresh"
goal = "Update README to mention the new CLI command"
context = "Ensure we mention the spec workflow."

tasks = [
    "Document the new command",
    "Provide an example spec file"
]

deliverables = [
    "README update summary"
]
        "#;

        let spec = AgentSpec::from_str(contents).expect("spec should parse");
        assert_eq!(spec.display_name(), "Docs refresh");
        assert!(spec.preview().contains("Goal: Update README"));

        let prompt = spec.to_prompt();
        assert!(prompt.contains("Primary Goal"));
        assert!(prompt.contains("Tasks"));
        assert!(prompt.contains("Deliverables"));
    }

    #[test]
    fn rejects_spec_without_goal() {
        let contents = r#"
tasks = ["Do the thing"]
        "#;
        let err = AgentSpec::from_str(contents).unwrap_err();
        let message = format!("{:?}", err);
        assert!(message.contains("goal"));
    }

    #[test]
    fn rejects_spec_without_tasks_or_deliverables() {
        let contents = r#"
goal = "Just saying hi"
        "#;
        let err = AgentSpec::from_str(contents).unwrap_err();
        assert!(format!("{}", err).contains("task"));
    }
}
