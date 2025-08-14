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

echo """Please confirm that the following tasks have been performed:
    - ran 1_release-version.sh
    - pushed changes to a new 'release/' branch
    - created & merged a PR
    - waited for the release-plz CI process to generate another PR
    - merged that PR
"""
read -p "Have you completed all the above tasks? (y/N): " confirm
if [[ ! "$confirm" =~ ^[Yy] ]]; then
    echo "Aborting."
    exit 1
fi

git tag "$TAG"
git push origin "$TAG"
