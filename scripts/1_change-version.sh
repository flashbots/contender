#!/bin/bash

# Check for version argument
if [ -z "$1" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.3.0"
    exit 1
fi

version="$1"
echo "version: $version"

# update workspace.package.version in Cargo.toml
# Use -i'' for macOS compatibility (GNU sed ignores the empty string, BSD sed requires it)
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/^version = \".*\"/version = \"$version\"/" Cargo.toml
else
    sed -i "s/^version = \".*\"/version = \"$version\"/" Cargo.toml
fi

echo "finished."
git status