#!/bin/sh
set -e # Exit immediately if a command exits with a non-zero status.

echo "Custom build script is running!"

# 1. Verify that the environment variable is set and not empty
if [ -z "$REVOLVE_TARGET_DIR" ]; then
  echo "Error: REVOLVE_TARGET_DIR is not set!" >&2
  exit 1
fi
if [ "$REVOLVE_PACKAGE_NAME" != "custom-build-project" ]; then
    echo "Error: REVOLVE_PACKAGE_NAME is wrong!" >&2
    exit 1
fi

echo "Target directory is: $REVOLVE_TARGET_DIR"
echo "Package name is: $REVOLVE_PACKAGE_NAME"
echo "Package version is: $REVOLVE_PACKAGE_VERSION"

# 2. Create the artifact that the revolve config expects
# The path must match the `source` in Cargo.toml
ARTIFACT_PATH="$REVOLVE_TARGET_DIR/custom-artifact.txt"
echo "Creating artifact at $ARTIFACT_PATH"
mkdir -p "$REVOLVE_TARGET_DIR"
echo "This artifact was created by the custom build script." > "$ARTIFACT_PATH"
echo "Artifact created successfully."