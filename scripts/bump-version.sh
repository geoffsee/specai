#!/bin/bash

# Version bump script for spec-ai workspace
# Usage: ./bump-version.sh <new-version>
# Example: ./bump-version.sh 0.4.9

set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <new-version>"
    echo "Example: $0 0.4.9"
    exit 1
fi

NEW_VERSION="$1"

# Validate version format (basic semver)
if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    echo "Error: Invalid version format. Expected semver (e.g., 0.4.9 or 0.4.9-beta.1)"
    exit 1
fi

# Get current version from workspace
CURRENT_VERSION=$(grep -E '^version = "[0-9]+\.[0-9]+\.[0-9]+"' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')

if [ -z "$CURRENT_VERSION" ]; then
    echo "Error: Could not detect current version from Cargo.toml"
    exit 1
fi

echo "Bumping version: $CURRENT_VERSION -> $NEW_VERSION"
echo ""

# Files to update
FILES=(
    "Cargo.toml"
    "crates/spec-ai-knowledge-graph/Cargo.toml"
    "crates/spec-ai/Cargo.toml"
    "crates/spec-ai-core/Cargo.toml"
    "crates/spec-ai-config/Cargo.toml"
    "crates/spec-ai-policy/Cargo.toml"
    "crates/spec-ai-plugin/Cargo.toml"
    "crates/spec-ai-cli/Cargo.toml"
    "crates/spec-ai-api/Cargo.toml"
)

# Update each file
for file in "${FILES[@]}"; do
    if [ -f "$file" ]; then
        # Replace version in workspace.package section and dependency version strings
        sed -i '' "s/version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/g" "$file"
        echo "Updated: $file"
    else
        echo "Warning: $file not found"
    fi
done

echo ""
echo "Version bump complete!"
echo ""

# Verify the changes
echo "Verifying changes..."
echo "Workspace version:"
grep -E '^version = ' Cargo.toml | head -1

echo ""
echo "Dependency versions in crates:"
grep -rh "version = \"$NEW_VERSION\"" crates/*/Cargo.toml | sort -u | head -5

echo ""
echo "Next steps:"
echo "  1. Review changes: git diff"
echo "  2. Build to verify: cargo build"
echo "  3. Commit: git add -A && git commit -m \"Bump version to $NEW_VERSION\""
echo "  4. Tag: git tag v$NEW_VERSION"
echo "  5. Publish crates in order (or use scripts/publish.sh):"
echo "     cargo publish -p spec-ai-knowledge-graph"
echo "     cargo publish -p spec-ai-config"
echo "     cargo publish -p spec-ai-policy"
echo "     cargo publish -p spec-ai-plugin"
echo "     cargo publish -p spec-ai-core"
echo "     cargo publish -p spec-ai-api"
echo "     cargo publish -p spec-ai-cli"
echo "     cargo publish -p spec-ai"
