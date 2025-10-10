#!/bin/bash

# Test script for formula update functionality
set -e

echo "Testing formula update functionality..."

# Create test directory
mkdir -p test-assets

# Create dummy binary files for testing
echo "Creating test binary files..."
echo "dummy-linux-x86_64-content" > test-assets/ai-gateway-linux-x86_64
echo "dummy-linux-aarch64-content" > test-assets/ai-gateway-linux-aarch64
echo "dummy-macos-x86_64-content" > test-assets/ai-gateway-macos-x86_64
echo "dummy-macos-aarch64-content" > test-assets/ai-gateway-macos-aarch64

# Make them executable
chmod +x test-assets/*

# Test the Ruby script
echo "Running formula update script..."
ruby scripts/update-formula.rb v1.0.0-test \
  test-assets/ai-gateway-linux-x86_64 \
  test-assets/ai-gateway-linux-aarch64 \
  test-assets/ai-gateway-macos-x86_64 \
  test-assets/ai-gateway-macos-aarch64

# Verify the formula was updated
echo "Verifying formula content..."
if grep -q "version \"1.0.0-test\"" Formula/ellora.rb; then
  echo "✓ Version updated correctly"
else
  echo "✗ Version not updated correctly"
  exit 1
fi

if grep -q "v1.0.0-test" Formula/ellora.rb; then
  echo "✓ Tag name included in URLs"
else
  echo "✗ Tag name not included in URLs"
  exit 1
fi

# Check if checksums are present (they should be different for each file)
checksum_count=$(grep -c "sha256" Formula/ellora.rb)
if [ "$checksum_count" -ge 4 ]; then
  echo "✓ Checksums calculated and included"
else
  echo "✗ Not enough checksums found"
  exit 1
fi

echo "✓ All tests passed!"

# Clean up
rm -rf test-assets