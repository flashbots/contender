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
sed -i "s/^version = \".*\"/version = \"$version\"/" Cargo.toml

echo "finished."
git status