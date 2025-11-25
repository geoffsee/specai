#!/bin/bash
set -e

# Publish spec-ai crates in dependency order
#
# Dependency graph:
#   spec-ai-config (no internal deps)
#   spec-ai-policy -> spec-ai-config
#   spec-ai-core   -> spec-ai-config, spec-ai-policy
#   spec-ai-api    -> spec-ai-core, spec-ai-config, spec-ai-policy
#   spec-ai        -> spec-ai-core, spec-ai-config, spec-ai-policy, spec-ai-api
#   spec-ai-cli    -> spec-ai

CRATES=(
    "spec-ai-config"
    "spec-ai-policy"
    "spec-ai-core"
    "spec-ai-api"
    "spec-ai-cli"
    "spec-ai"
)

# Time to wait between publishes for crates.io index to update
WAIT_SECONDS=30

DRY_RUN=false
if [[ "$1" == "--dry-run" ]]; then
    DRY_RUN=true
    echo "=== DRY RUN MODE ==="
fi

echo "Publishing crates in order:"
for crate in "${CRATES[@]}"; do
    echo "  - $crate"
done
echo ""

for i in "${!CRATES[@]}"; do
    crate="${CRATES[$i]}"
    echo "=== Publishing $crate ==="

    if $DRY_RUN; then
        cargo publish -p "$crate" --dry-run
    else
        cargo publish -p "$crate"

        # Wait between publishes (except for the last one)
        if [[ $i -lt $((${#CRATES[@]} - 1)) ]]; then
            echo "Waiting ${WAIT_SECONDS}s for crates.io index to update..."
            sleep $WAIT_SECONDS
        fi
    fi

    echo ""
done

echo "=== All crates published successfully ==="
