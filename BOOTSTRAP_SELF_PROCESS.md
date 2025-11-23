# Bootstrap Self Process

This document maps the current bootstrap-self flow and plugins in `spec-ai`.

## Upcoming Changes (design staged)

- Add `/refresh` to rerun bootstrap with caching, distinct from first-use `/init`.
- Persist per-file token stats and embeddings in DuckDB for reuse across sessions.
- Reuse cached tokenization when file hashes match; only re-tokenize changed files.
- Link per-file embeddings into the graph so downstream agents can query them immediately.
- Keep existing defaults (rust-cargo, toak-tokenizer, universal-code) but allow explicit plugin selection.

## Pipeline Control Flow

```mermaid
flowchart TD
  A["CLI /init or BootstrapSelf::run_with_plugins"] --> B["Resolve repo root + session context"]
  B --> C["Register rust-cargo, toak-tokenizer, universal-code"]
  C --> D{Explicit plugin list?}
  D -->|yes| E["Load plugins by name"]
  D -->|no| F["Auto-enable plugins via should_activate"]
  E --> G["Active plugin set"]
  F --> G
  G --> H["Run each plugin in order"]
  H --> I["Aggregate node/edge counts, phases, metadata; keep first root node"]
  I --> J["Return BootstrapOutcome to CLI"]
```

## Plugin Responsibilities and Graph Writes

```mermaid
graph TD
  subgraph "Rust-Cargo Plugin"
    rc_repo["Repository Entity: name, version, edition, dependency counts"]
    rc_comp["Component Entity xN: path + stats + sampled files"]
    rc_doc["RepositoryDocument Fact xM: path + preview + size"]
    rc_manifest["CargoManifest Concept: deps/dev-deps/build-deps/features"]
    rc_comp -->|PART_OF| rc_repo
    rc_doc -->|RELATES_TO documents| rc_repo
    rc_manifest -->|DEPENDS_ON builds| rc_repo
  end

  subgraph "Toak-Tokenizer Plugin"
    tt_repo["Repository Entity: path + token_profile summary"]
    tt_token["TokenFootprint Concept: raw/cleaned totals + top files"]
    tt_file["TokenizedFile Concept xN: per-file tokens + cached flag + embedding_id"]
    tt_token -->|RELATES_TO tokenized_with| tt_repo
    tt_file -->|RELATES_TO tokenized_file| tt_repo
    tt_file -->|RELATES_TO summarized_in| tt_token
  end

  subgraph "Universal-Code Plugin"
    uc_repo["Repository Entity: intent, languages, frameworks, complexity"]
    uc_comp["Component Entity xN: type + path"]
    uc_doc["Documentation Fact xK: title + path"]
    uc_comp -->|PART_OF component_of| uc_repo
    uc_doc -->|RELATES_TO documents| uc_repo
  end
```

## Example Graph (Simple Repository)

```mermaid
graph TD
  repo["Repository (rust-cargo)"]
  comp["Component: src/"]
  doc["Documentation: README.md"]
  manifest["CargoManifest"]

  tokens_repo["Repository (token profile)"]
  footprint["TokenFootprint"]
  token_file["TokenizedFile: src/lib.rs"]

  uc_repo["Repository (universal-code)"]
  uc_doc["Documentation: .SPEC-AI.md"]

  comp -->|PART_OF| repo
  doc -->|RELATES_TO documents| repo
  manifest -->|DEPENDS_ON builds| repo

  footprint -->|RELATES_TO tokenized_with| tokens_repo
  token_file -->|RELATES_TO tokenized_file| tokens_repo
  token_file -->|RELATES_TO summarized_in| footprint

  uc_doc -->|RELATES_TO documents| uc_repo
```

## Plugin Behavior Details

- `rust-cargo`
  - Activates when `Cargo.toml` exists.
  - Phases: survey layout, index docs/specs, extract Cargo manifest, link graph.
  - Limits: components capped at 12, documents at 8; samples up to 5 files per component.
  - Outputs: repository node plus component, document, and manifest nodes with typed edges.

- `toak-tokenizer`
  - Activates when a `.git` directory exists.
  - Phases: discover tracked files (via `git ls-files` fallback to walk), clean/redact (`toak_rs::clean_and_redact`), count tokens (`count_tokens`), hash + cache per-file stats, build hashed embeddings, summarize.
  - Limits: max 200 files, 200KB per file, ignores common binary extensions and build/temp dirs; keeps top 8 files by cleaned token count.
  - Outputs: repository node with token profile, `TokenFootprint` concept, and per-file `TokenizedFile` nodes (with cached flag, token counts, bytes, embedding_id) linked back to the repo and footprint. Persists per-file stats and embeddings in DuckDB for cache-aware refreshes.

- `/refresh`
  - Re-runs bootstrap with caching enabled so tokenized files and embeddings reuse DuckDB data when unchanged.

- `universal-code`
  - Activates when a VCS marker, code files, or common manifests are present.
  - Phases: classify files, intent analysis, semantic analysis, build knowledge graph.
  - Limits: scans up to 1000 files, caps components at 15 and documents at 8; ignores common build/cache directories.
  - Outputs: repository node plus component/document nodes and writes a `.SPEC-AI.md` summary to the repo root. If tokenized file cache exists, adds per-repo and per-component token profiles sourced from DuckDB without re-tokenizing.

## Data Aggregation Notes

- `BootstrapSelf` keeps the first plugin’s `root_node_id` as the repository identifier in the final `BootstrapOutcome`, but each plugin writes its own repository node to the graph.
- Node/edge counts in the outcome are totals across all plugins; phases are concatenated in plugin execution order.
