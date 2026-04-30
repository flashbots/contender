#!/bin/bash


set -e


# Enumerate crates from crates/ directory (kebab-case)
CRATES_DIR="/Users/brock/code/contender/crates"
CRATES=""
CRATE_ARR=()
for dir in "$CRATES_DIR"/*/; do
    [ -d "$dir" ] || continue
    crate_name=$(basename "$dir")
    CRATE_ARR+=("$crate_name")
done

if [ ${#CRATE_ARR[@]} -eq 0 ]; then
    echo "No crates found in $CRATES_DIR"
    exit 1
fi

echo "Select a crate to tag:"
select CRATE in "${CRATE_ARR[@]}"; do
    if [ -n "$CRATE" ]; then
        break
    fi
done


# Map crate to directory
CRATE_DIR="/Users/brock/code/contender/crates/${CRATE//-/_}"
if [ ! -d "$CRATE_DIR" ]; then
    # fallback for kebab-case dirs
    CRATE_DIR="/Users/brock/code/contender/crates/$CRATE"
fi
CARGO_TOML="$CRATE_DIR/Cargo.toml"
if [ ! -f "$CARGO_TOML" ]; then
    echo "Cargo.toml not found for crate $CRATE"
    exit 1
fi

# Extract current version
CURRENT_VERSION=$(grep '^version' "$CARGO_TOML" | head -n1 | sed -E 's/version *= *"([0-9]+\.[0-9]+\.[0-9]+)"/\1/')
if [[ ! "$CURRENT_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Could not determine current version for $CRATE"
    exit 1
fi

# Preview version bumps for the selected crate
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"
PATCH_NEXT=$((PATCH + 1))
MINOR_NEXT=$((MINOR + 1))
MAJOR_NEXT=$((MAJOR + 1))

PATCH_VERSION="v$MAJOR.$MINOR.$PATCH_NEXT"
MINOR_VERSION="v$MAJOR.$MINOR_NEXT.0"
MAJOR_VERSION="v$MAJOR_NEXT.0.0"

echo "Current version: v$CURRENT_VERSION"
echo "Select version bump type:"
echo "1) patch -> $PATCH_VERSION"
echo "2) minor -> $MINOR_VERSION"
echo "3) major -> $MAJOR_VERSION"

while true; do
    read -p "Enter choice [1-3]: " bump_choice
    case $bump_choice in
        1|patch)
            BUMP="patch"
            NEW_VERSION="${PATCH_VERSION#v}"
            break
            ;;
        2|minor)
            BUMP="minor"
            NEW_VERSION="${MINOR_VERSION#v}"
            break
            ;;
        3|major)
            BUMP="major"
            NEW_VERSION="${MAJOR_VERSION#v}"
            break
            ;;
        *)
            echo "Invalid choice. Please enter 1, 2, or 3."
            ;;
    esac
done

# Map crate to directory
CRATE_DIR="/Users/brock/code/contender/crates/${CRATE//-/_}"
if [ ! -d "$CRATE_DIR" ]; then
    # fallback for kebab-case dirs
    CRATE_DIR="/Users/brock/code/contender/crates/$CRATE"
fi
CARGO_TOML="$CRATE_DIR/Cargo.toml"
if [ ! -f "$CARGO_TOML" ]; then
    echo "Cargo.toml not found for crate $CRATE"
    exit 1
fi

# Extract current version
CURRENT_VERSION=$(grep '^version' "$CARGO_TOML" | head -n1 | sed -E 's/version *= *"([0-9]+\.[0-9]+\.[0-9]+)"/\1/')
if [[ ! "$CURRENT_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Could not determine current version for $CRATE"
    exit 1
fi

# Calculate new version
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"
case "$BUMP" in
    patch)
        PATCH=$((PATCH + 1))
        ;;
    minor)
        MINOR=$((MINOR + 1))
        PATCH=0
        ;;
    major)
        MAJOR=$((MAJOR + 1))
        MINOR=0
        PATCH=0
        ;;
esac
NEW_VERSION="$MAJOR.$MINOR.$PATCH"
TAG="${CRATE}_v${NEW_VERSION}"
echo "Will create tag: $TAG"

confirm() {
        read -p "$1 (y/N): " confirm
        if [[ ! "$confirm" =~ ^[Yy] ]]; then
                echo "Aborting."
                exit 1
        fi
}

echo """Please confirm that the following tasks have been performed:
        - run 1_change-version.sh for $CRATE
        - check Cargo.lock to make sure the versions have been updated (run 'cargo build' if they haven't)
        - push changes to a new 'release/' branch
        - create & merge a PR w/ the new version changes
        - ensure $CRATE version in Cargo.toml is $NEW_VERSION
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
