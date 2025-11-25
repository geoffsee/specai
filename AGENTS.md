# Repository Guidelines

## Project Structure & Module Organization
- `crates/` hosts the workspace members:
  - `spec-ai-cli/` (binary entrypoint),
  - `spec-ai-core/` (agent runtime + tools),
  - `spec-ai-config/` (config + persistence),
  - `spec-ai-policy/` (policy + plugin system),
  - `spec-ai-api/` (HTTP/mesh server),
  - `spec-ai/` (public re-export crate).
- `tests/` contains integration suites; `specs/` houses reusable `.spec` plans; `examples/` contains reference code and configuration samples; `docs/` contains all documentation.
- `examples/code/` contains example Rust code demonstrating various features.
- `examples/configs/` contains sample configuration files for different providers and setups.
- `docs/` contains architecture docs (`ARCHITECTURE.md`), configuration guide (`CONFIGURATION.md`), setup instructions (`SETUP.md`), and other documentation.
- Root assets include the main config (`spec-ai.config.toml`) and the `Containerfile`.

## Build, Test, and Development Commands
- `cargo build` / `cargo test`: compile or run tests using the system DuckDB (fast builds).
- `cargo build --features bundled` / `cargo test --features bundled`: compile or run tests using the embedded DuckDB (for CI or systems without DuckDB installed).
- `cargo run -p spec-ai-cli -- --config ./custom.toml`: launch the agent with the current directory config; `-c`/`--config` overrides.
- `podman build -t spec-ai .` and `podman run --rm spec-ai --help` (Docker equivalent) exercise the containerized workflow.

## Coding Style & Naming Conventions
- Always run `cargo fmt` (four-space default) and `cargo clippy` before merging to maintain idiomatic Rust.
- Use `snake_case` for functions and fields, `PascalCase` for structs/enums, and `SCREAMING_SNAKE_CASE` for constants.
- Place new `.spec` files in `specs/` and give them descriptive names (e.g., `docs_refresh.spec`) so automation can locate them.

## Testing Guidelines
- Default test run: `cargo test --all-targets`.
- Feature-specific suites: `cargo test --features api`, `cargo test --lib plugin`, `cargo test --lib policy`, `cargo test --test policy_integration_tests`.
- Validate agent plans with `spec-ai run specs/smoke.spec` and include any GraalVM/Tesseract setup steps needed for file extraction.
- Integration tests in `tests/` follow the `*_tests.rs` pattern and focus on persistence and agent flow behavior.

## Commit & Pull Request Guidelines
- Keep commits short, present-tense, and descriptive (e.g., `add run spec subcommand`); mention the subsystem when helpful.
- PR descriptions should summarize the change, link related issues, list commands executed, and note native dependencies touched.
- Attach spec output, config edits, or screenshots whenever configuration flows or agent prompts change.

## Agent & Spec Notes
- Define agents in `spec-ai.config.toml` (or `~/.spec-ai/spec-ai.config.toml`) under `[agents.<name>]` with `prompt`, `temperature`, and tool allow/deny lists as shown in README.
- Use `/spec run specs/<file>.spec` (or `/spec specs/<file>.spec`) inside the CLI; every `.spec` needs a `goal` plus `tasks` or `deliverables`.
- Update `crates/spec-ai-config/src/config/registry.rs` when agent-switching behavior changes and rerun `cargo fmt`/`cargo clippy` afterward.
