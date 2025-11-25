#!/bin/bash
set -euo pipefail

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
    "spec-ai"
    "spec-ai-cli"
)

# Time to wait between publishes for crates.io index to update
WAIT_SECONDS=30

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
    DRY_RUN=true
    echo "=== DRY RUN MODE ==="
fi

# Get workspace version from root Cargo.toml
VERSION=$(
    sed -n '/\[workspace\.package\]/,/\[/p' Cargo.toml \
    | grep '^version' \
    | head -1 \
    | sed 's/.*"\(.*\)"/\1/'
)
echo "Workspace version: $VERSION"
echo ""

# Check if a crate version is already published
is_published() {
    local crate=$1
    local version=$2
    local status

    status=$(curl -s -o /dev/null -w "%{http_code}" "https://crates.io/api/v1/crates/$crate/$version")
    [[ "$status" == "200" ]]
}

echo "Publishing crates in order:"
for crate in "${CRATES[@]}"; do
    echo "  - $crate"
done
echo ""

published_count=0
skipped_count=0

for i in "${!CRATES[@]}"; do
    crate="${CRATES[$i]}"
    echo "=== $crate ==="

    if is_published "$crate" "$VERSION"; then
        echo "Already published at version $VERSION, skipping..."
        ((skipped_count++))
        echo ""
        continue
    fi

    echo "Publishing $crate@$VERSION..."

    if $DRY_RUN; then
        cargo publish -p "$crate" --dry-run
    else
        # Run publish, and if it fails, re-check crates.io to see
        # whether the version actually ended up published anyway
        if cargo publish -p "$crate"; then
            ((published_count++))
        else
            echo "cargo publish failed for $crate@$VERSION, checking crates.io..." >&2
            if is_published "$crate" "$VERSION"; then
                echo "Version $VERSION for $crate is now visible on crates.io; treating as success."
                ((skipped_count++))
            else
                echo "Version $VERSION for $crate is NOT published; failing the job." >&2
                exit 1
            fi
        fi

        # Wait between publishes (except for the last one)
        if [[ $i -lt $((${#CRATES[@]} - 1)) ]]; then
            echo "Waiting ${WAIT_SECONDS}s for crates.io index to update..."
            sleep "$WAIT_SECONDS"
        fi
    fi

    echo ""
done

echo "=== Done ==="
echo "Published: $published_count, Skipped (already published / race): $skipped_count"
