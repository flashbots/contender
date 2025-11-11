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
    - run 1_change-version.sh
    - check Cargo.lock to make sure the versions have been updated (run 'cargo build' if they haven't)
    - push changes to a new 'release/' branch
    - create & merge a PR w/ the new version changes
"""
confirm "Have you completed all the above tasks?"

echo "Enter a message to attach to the tag. Press Enter twice to finish or CTRL-C to quit:"
TAG_MESSAGE=""
while true; do
    IFS= read -r line
    if [ -z "$line" ]; then
        break
    fi
    TAG_MESSAGE="${TAG_MESSAGE}${line}\n"
done

if [ -z "$TAG_MESSAGE" ]; then
    echo "No message entered. Aborting."
    exit 1
fi

git tag -a "$TAG" -m "$(echo -e "$TAG_MESSAGE")"

confirm "Do you want to push this tag to the remote origin?"

git push origin "$TAG"
