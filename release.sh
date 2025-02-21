#!/bin/bash

set -e

# Check if version argument is provided
if [ -z "$1" ]; then
    echo "Please provide version argument (major|minor|patch)"
    exit 1
fi

VERSION_TYPE=$1

# Install cargo-edit if not already installed
if ! cargo set-version --help >/dev/null 2>&1; then
    echo "Installing cargo-edit..."
    cargo install cargo-edit
fi

# Get current version from gateway Cargo.toml
CURRENT_VERSION=$(grep -m1 '^version = ' gateway/Cargo.toml | cut -d '"' -f2)

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
(cd udfs && cargo set-version $NEW_VERSION)
(cd gateway && cargo set-version $NEW_VERSION)

# Generate CHANGELOG
echo "Generating CHANGELOG..."
npx standard-version --release-as $NEW_VERSION --skip.tag true

# Create git tag
echo "Creating git tag v$NEW_VERSION..."
git tag -a "v$NEW_VERSION" -m "Release v$NEW_VERSION"

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
gh release create "v$NEW_VERSION" \
    --title "Release v$NEW_VERSION" \
    --notes-file CHANGELOG.md \
    $TEMP_DIR/langdb_udf-x86_64 \
    $TEMP_DIR/langdb_udf-aarch64 \
    $TEMP_DIR/langdb_gateway-x86_64 \
    $TEMP_DIR/langdb_gateway-aarch64

# Cleanup
rm -rf $TEMP_DIR

# Create and push PR for version bump and CHANGELOG
echo "Creating PR for version bump..."
BRANCH_NAME="release/v$NEW_VERSION"
git checkout -b $BRANCH_NAME
git add CHANGELOG.md core/Cargo.toml udfs/Cargo.toml gateway/Cargo.toml
git commit -m "chore: release v$NEW_VERSION"
git push origin $BRANCH_NAME

gh pr create \
    --title "Release v$NEW_VERSION" \
    --body "Automated PR for version v$NEW_VERSION release" \
    --base main \
    --head $BRANCH_NAME

echo "Release process completed successfully!"

