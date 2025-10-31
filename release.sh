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

# Update version in Cargo.toml files
echo "Updating version to $NEW_VERSION in Cargo.toml files..."
(cd core && cargo set-version $NEW_VERSION)
(cd gateway && cargo set-version $NEW_VERSION)
(cd guardrails && cargo set-version $NEW_VERSION)

# Install standard-version if not already installed
if ! command -v npx &> /dev/null; then
    echo "Installing npx..."
    npm install -g npx
fi

# Generate CHANGELOG
echo "Generating CHANGELOG..."
npx standard-version --release-as "$NEW_VERSION" --tag-prefix "" --skip.tag true

git add CHANGELOG.md core/Cargo.toml gateway/Cargo.toml guardrails/Cargo.toml
git commit -m "chore: release v$NEW_VERSION"
git push origin main

gh pr create \
    --title "Release v$NEW_VERSION" \
    --body "Automated PR for version v$NEW_VERSION release" \
    --base main \
    --head main

