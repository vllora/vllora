#!/bin/bash

set -e

# Check if version argument is provided
if [ -z "$1" ]; then
    echo "Please provide version argument (major|minor|patch) or --publish VERSION"
    exit 1
fi

VERSION_TYPE=$1

# Get current version from core/Cargo.toml
CURRENT_VERSION=$(grep '^version = ' core/Cargo.toml | sed 's/version = "\(.*\)"/\1/')

if [ -z "$CURRENT_VERSION" ]; then
    echo "Error: Could not determine current version from core/Cargo.toml"
    exit 1
fi

# Calculate new version based on version type
case $VERSION_TYPE in
    major)
        NEW_VERSION=$(echo $CURRENT_VERSION | awk -F. '{$1 = $1 + 1; $2 = 0; $3 = 0} 1' OFS=.)
        ;;
    minor)
        NEW_VERSION=$(echo $CURRENT_VERSION | awk -F. '{$2 = $2 + 1; $3 = 0} 1' OFS=.)
        ;;
    patch)
        NEW_VERSION=$(echo $CURRENT_VERSION | awk -F. '{$3 = $3 + 1} 1' OFS=.)
        ;;
    *)
        echo "Invalid version type. Use major, minor, or patch"
        exit 1
        ;;
esac

# Delete all prerelease tags for the new version
echo "Deleting prerelease tags for v$NEW_VERSION..."
PRERELEASE_TAGS=$(git tag -l "v$NEW_VERSION-prerelease-*" 2>/dev/null || true)
if [ -n "$PRERELEASE_TAGS" ]; then
    echo "$PRERELEASE_TAGS" | while read tag; do
        if [ -n "$tag" ]; then
            echo "Deleting local tag: $tag"
            git tag -d "$tag" 2>/dev/null || true
            echo "Deleting remote tag: $tag"
            git push origin --delete "$tag" 2>/dev/null || true
        fi
    done
    echo "All prerelease tags for v$NEW_VERSION have been deleted."
else
    echo "No prerelease tags found for v$NEW_VERSION."
fi

# Update version in Cargo.toml files
echo "Updating version to $NEW_VERSION in Cargo.toml files..."
(cd core && cargo set-version $NEW_VERSION)
(cd gateway && cargo set-version $NEW_VERSION)
(cd guardrails && cargo set-version $NEW_VERSION)
(cd llm && cargo set-version $NEW_VERSION)
(cd telemetry && cargo set-version $NEW_VERSION)

cargo build --release

git add CHANGELOG.md core/Cargo.toml gateway/Cargo.toml guardrails/Cargo.toml telemetry/Cargo.toml llm/Cargo.toml Cargo.lock

# Install standard-version if not already installed
if ! command -v npx &> /dev/null; then
    echo "Installing npx..."
    npm install -g npx
fi

# Generate CHANGELOG
echo "Generating CHANGELOG..."
npx standard-version --release-as "$NEW_VERSION" --tag-prefix "" --skip.tag true --commit-all

git push origin main

git tag v$NEW_VERSION
git push origin v$NEW_VERSION

cargo publish -p vllora_telemetry
cargo publish -p vllora_llm
cargo publish -p vllora_core
cargo publish -p vllora_guardrails
cargo publish -p vllora

