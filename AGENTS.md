# Repository Guidelines

## Project Structure & Module Organization
- `src/` hosts the CLI entry (`main.rs`), library surface (`lib.rs`), config helpers, persistence layer, and shared types.
- `tests/` contains integration suites; `specs/` plus `scripts/` house reusable `.spec` plans and execution helpers; `examples/` and `docs/` track reference code and prose.
- Root assets include config samples (`config.*.example.toml`, `spec-ai.config.toml`), the `Containerfile`, and the bundled DuckDB helper in `libduckdb`.

## Build, Test, and Development Commands
- `cargo binstall spec-ai --features bundled`: install the CLI with the bundled DuckDB runtime for fast iteration.
- `cargo build/test --features bundled`: compile or run tests using the embedded DuckDB (per README guidance).
- `./setup_duckdb.sh && source duckdb_env.sh`: switch to a system DuckDB build when the bundled option is insufficient before rerunning `cargo build`/`cargo test`.
- `cargo run -- --config ./custom.toml` (or `cargo run`): launch the agent with the current directory config; `-c`/`--config` overrides.
- `podman build -t spec-ai .` and `podman run --rm spec-ai --help` (Docker equivalent) exercise the containerized workflow.

## Coding Style & Naming Conventions
- Always run `cargo fmt` (four-space default) and `cargo clippy` before merging to maintain idiomatic Rust.
- Use `snake_case` for functions and fields, `PascalCase` for structs/enums, and `SCREAMING_SNAKE_CASE` for constants.
- Place new `.spec` files in `specs/` and give them descriptive names (e.g., `docs_refresh.spec`) so automation can locate them.

## Testing Guidelines
- Default test run: `cargo test --all-targets`.
- Feature-specific suites: `cargo test --features api`, `cargo test --lib plugin`, `cargo test --lib policy`, `cargo test --test policy_integration_tests`.
- Validate agent plans with `scripts/run_specs.sh specs/` (or `scripts/run_specs.sh specs/smoke.spec`) and include any GraalVM/Tesseract setup steps needed for file extraction.
- Integration tests in `tests/` follow the `*_tests.rs` pattern and focus on persistence and agent flow behavior.

## Commit & Pull Request Guidelines
- Keep commits short, present-tense, and descriptive (e.g., `add run spec subcommand`); mention the subsystem when helpful.
- PR descriptions should summarize the change, link related issues, list commands executed, and note native dependencies touched.
- Attach spec output, config edits, or screenshots whenever configuration flows or agent prompts change.

## Agent & Spec Notes
- Define agents in `spec-ai.config.toml` (or `~/.spec-ai/spec-ai.config.toml`) under `[agents.<name>]` with `prompt`, `temperature`, and tool allow/deny lists as shown in README.
- Use `/spec run specs/<file>.spec` (or `/spec specs/<file>.spec`) inside the CLI; every `.spec` needs a `goal` plus `tasks` or `deliverables`.
- Update `src/config/registry.rs` when agent-switching behavior changes and rerun `cargo fmt`/`cargo clippy` afterward.
