#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: scripts/run_specs.sh [<spec-or-dir> ...]

Provide zero or more `.spec` files or directories containing specs.
Each spec is piped into the SpecAI CLI via `/spec run <file>` followed by `/quit`.
Set SPEC_AI_CMD="cargo run --quiet --" to override the binary invocation.
EOF
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_BIN="${REPO_ROOT}/target/debug/spec-ai"
DEFAULT_SPEC="${REPO_ROOT}/specs/smoke.spec"

if [[ -z "${SPEC_AI_CMD:-}" ]]; then
    if [[ ! -x "$DEFAULT_BIN" ]]; then
        echo "Building spec-ai binary..." >&2
        (cd "$REPO_ROOT" && cargo build --quiet)
    fi
    CLI_LAUNCH=("$DEFAULT_BIN")
else
    CLI_LAUNCH=(bash -lc "$SPEC_AI_CMD")
fi

if [[ -z "${RUST_LOG:-}" ]]; then
    export RUST_LOG="agent_timing=info"
fi

collect_specs() {
    local target="$1"
    if [[ -d "$target" ]]; then
        find "$target" -type f -name '*.spec' | sort
    else
        printf "%s\n" "$target"
    fi
}

SPEC_FILES=()
if [[ $# -eq 0 ]]; then
    if [[ ! -f "$DEFAULT_SPEC" ]]; then
        echo "Default spec not found at '$DEFAULT_SPEC'. Provide explicit specs or create the smoke test file." >&2
        exit 1
    fi
    SPEC_FILES+=("$DEFAULT_SPEC")
else
    while [[ $# -gt 0 ]]; do
        while IFS= read -r spec; do
            [[ -z "$spec" ]] && continue
            SPEC_FILES+=("$spec")
        done < <(collect_specs "$1")
        shift
    done
fi

if [[ ${#SPEC_FILES[@]} -eq 0 ]]; then
    echo "No .spec files found in provided arguments." >&2
    exit 1
fi

run_spec() {
    local spec="$1"
    if [[ ! -f "$spec" ]]; then
        echo "Skipping '$spec' (file not found)." >&2
        return 1
    fi
    if [[ "${spec##*.}" != "spec" ]]; then
        echo "Skipping '$spec' (expected .spec extension)." >&2
        return 1
    fi
    local abs_spec
    abs_spec="$(cd "$(dirname "$spec")" && pwd)/$(basename "$spec")"
    echo "=== Running spec: $abs_spec ==="
    printf "/spec run %s\n/quit\n" "$abs_spec" | (cd "$REPO_ROOT" && "${CLI_LAUNCH[@]}")
}

status=0
for spec in "${SPEC_FILES[@]}"; do
    if ! run_spec "$spec"; then
        status=1
    fi
done

exit $status
