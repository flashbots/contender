#!/bin/bash

set -e

TAG_CACHE="/tmp/contender-tag-cache.txt"

TAGS=()
while IFS= read -r line; do
  TAGS+=("$line")
done < "$TAG_CACHE"

echo "Staged for tagging:"
for tag in "${TAGS[@]}"; do
    echo "$tag"
done
