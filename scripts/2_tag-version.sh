#!/bin/bash

set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <tag>"
    exit 1
fi

TAG="$1"
echo "tag: $TAG"

if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: Tag must start with 'v' and follow semantic versioning (e.g., v1.2.3)"
    exit 1
fi

confirm() {
    read -p "$1 (y/N): " confirm
    if [[ ! "$confirm" =~ ^[Yy] ]]; then
        echo "Aborting."
        exit 1
    fi
}

echo """Please confirm that the following tasks have been performed:
    - ran 1_release-version.sh
    - pushed changes to a new 'release/' branch
    - created & merged a PR
    - waited for the release-plz CI process to generate another PR
    - merged that PR
"""
confirm "Have you completed all the above tasks?"

git tag "$TAG"
confirm "Do you want to push this tag to the remote origin?"

git push origin "$TAG"
