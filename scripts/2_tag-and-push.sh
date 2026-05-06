#!/bin/bash

set -e

TAG_CACHE="/tmp/contender-tag-cache.txt"

if [ ! -f "$TAG_CACHE" ]; then
    echo "Tag cache file not found at $TAG_CACHE. Please run 1_set-version.sh first to prepare tag names."
    exit 1
fi

TAGS=()
while IFS= read -r line; do
  TAGS+=("$line")
done < "$TAG_CACHE"

if [ ${#TAGS[@]} -eq 0 ]; then
    echo "No tags found in ${TAG_CACHE}. Please run 1_set-version.sh first to prepare tag names."
    exit 1
fi

echo "Staged for tagging:"
for tag in "${TAGS[@]}"; do
    echo "$tag"
done

echo
read -p "Create tags now? (y/N): " confirm_tag
if [[ "$confirm_tag" =~ ^[Yy] ]]; then
    for t in "${TAGS[@]}"; do
        # Checks if tag exists; if not, creates it
        git rev-parse "$t" > /dev/null 2>&1 || git tag "$t"
        echo "Created tag: $t"
    done
else
    echo "No tags were created."
fi

echo
read -p "Push all tags to the remote origin? (y/N): " confirm_push
if [[ "$confirm_push" =~ ^[Yy] ]]; then
    for t in "${TAGS[@]}"; do
        echo "Pushing tag: $t"
        git push origin "$t"
    done
    echo "All tags pushed."
else
    echo "Tags were created locally but not pushed. To push later, run: git push origin <tag-name>"
fi
