#!/bin/bash

# Define the crate names
crates=(
    contender_bundle_provider
    contender_cli
    contender_core
    contender_engine_provider
    contender_sqlite
    contender_testfile
    contender_report
)

# Check for version argument
if [ -z "$1" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.3.0"
    exit 1
fi

version="$1"
echo "version: $version"

# Build the command
cmd="release-plz set-version"
for crate in "${crates[@]}"; do
    cmd+=" ${crate}@${version}"
done

# Execute the command
eval "$cmd"

echo "finished."
git status