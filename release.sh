#!/bin/bash

set -e

# Check if version argument is provided
if [ -z "$1" ]; then
    echo "Please provide version argument (major|minor|patch)"
    exit 1
fi

VERSION_TYPE=$1

# Install standard-version if not already installed
if ! command -v npx &> /dev/null; then
    echo "Installing npx..."
    npm install -g npx
fi

# Generate CHANGELOG and bump version
echo "Generating CHANGELOG and bumping version..."
npx standard-version --release-as $VERSION_TYPE

# Get the new version from package.json (or you can parse from git tags)
NEW_VERSION=$(git describe --tags --abbrev=0)

# Build the artifacts
echo "Building artifacts..."
make build_udfs
make build_gateways

# Create temporary directory for release assets
TEMP_DIR=$(mktemp -d)

# Copy and rename binaries for different architectures
echo "Preparing release artifacts..."
cp target/x86_64-unknown-linux-gnu/release/langdb_udf $TEMP_DIR/langdb_udf-x86_64
cp target/aarch64-unknown-linux-gnu/release/langdb_udf $TEMP_DIR/langdb_udf-aarch64

cp target/x86_64-unknown-linux-gnu/release/langdb_gateway $TEMP_DIR/langdb_gateway-x86_64
cp target/aarch64-unknown-linux-gnu/release/langdb_gateway $TEMP_DIR/langdb_gateway-aarch64

# Create GitHub release and upload assets
echo "Creating GitHub release..."
gh release create $NEW_VERSION \
    --title "Release $NEW_VERSION" \
    --notes-file CHANGELOG.md \
    $TEMP_DIR/langdb_udf-x86_64 \
    $TEMP_DIR/langdb_udf-aarch64 \
    $TEMP_DIR/langdb_gateway-x86_64 \
    $TEMP_DIR/langdb_gateway-aarch64

# Cleanup
rm -rf $TEMP_DIR

# Create and push PR for version bump and CHANGELOG
echo "Creating PR for version bump..."
BRANCH_NAME="release/$NEW_VERSION"
git checkout -b $BRANCH_NAME
git add CHANGELOG.md package.json
git commit -m "chore: release $NEW_VERSION"
git push origin $BRANCH_NAME

gh pr create \
    --title "Release $NEW_VERSION" \
    --body "Automated PR for version $NEW_VERSION release" \
    --base main \
    --head $BRANCH_NAME

echo "Release process completed successfully!"

