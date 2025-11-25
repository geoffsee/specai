#!/bin/bash
set -e

# Build and publish the CI base image to GitHub Container Registry
#
# Prerequisites:
#   - Docker installed and running
#   - Logged into ghcr.io: echo $GITHUB_TOKEN | docker login ghcr.io -u USERNAME --password-stdin
#
# Usage:
#   ./scripts/publish-ci-image.sh           # Build and push
#   ./scripts/publish-ci-image.sh --local   # Build locally only (no push)

IMAGE_NAME="ghcr.io/geoffsee/spec-ai-ci"
TAG="latest"

LOCAL_ONLY=false
if [[ "$1" == "--local" ]]; then
    LOCAL_ONLY=true
    echo "=== LOCAL BUILD ONLY ==="
fi

echo "Building CI image: ${IMAGE_NAME}:${TAG}"
echo ""

docker build \
    --platform linux/amd64 \
    -t "${IMAGE_NAME}:${TAG}" \
    -f .github/CI.Dockerfile \
    .

echo ""
echo "Build complete: ${IMAGE_NAME}:${TAG}"

if $LOCAL_ONLY; then
    echo "Skipping push (--local flag set)"
else
    echo "Pushing to ghcr.io..."
    docker push "${IMAGE_NAME}:${TAG}"
    echo ""
    echo "Published: ${IMAGE_NAME}:${TAG}"
fi