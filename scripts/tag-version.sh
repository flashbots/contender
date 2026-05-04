#!/bin/bash


set -e



# Enumerate crates by package name and map to their directory (POSIX-compatible)
CRATES_DIR="/Users/brock/code/contender/crates"
PKG_ARR=()
DIR_ARR=()
for dir in "$CRATES_DIR"/*/; do
    [ -d "$dir" ] || continue
    CARGO_TOML="$dir/Cargo.toml"
    if [ -f "$CARGO_TOML" ]; then
        PKG_NAME=$(grep '^name =' "$CARGO_TOML" | head -n1 | sed -E 's/name = "([^"]+)"/\1/')
        if [ -n "$PKG_NAME" ]; then
            PKG_ARR+=("$PKG_NAME")
            DIR_ARR+=("${dir%/}")
        fi
    fi
done

if [ ${#PKG_ARR[@]} -eq 0 ]; then
    echo "No crates found in $CRATES_DIR"
    exit 1
fi

echo "Select a crate to tag (by package name):"
select CRATE in "${PKG_ARR[@]}"; do
    if [ -n "$CRATE" ]; then
        IDX=$((REPLY-1))
        CRATE_DIR="${DIR_ARR[$IDX]}"
        # Remove trailing slash if present
        CRATE_DIR="${CRATE_DIR%/}"
        CARGO_TOML="$CRATE_DIR/Cargo.toml"
        if [ ! -f "$CARGO_TOML" ]; then
            echo "Cargo.toml not found for crate $CRATE"
            echo "Checked path: $CARGO_TOML"
            echo "Directory contents:"
            ls -l "$CRATE_DIR"
            exit 1
        fi
        break
    fi
done



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


# Update the version in the selected crate's Cargo.toml (after bump selection)
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"
else
    sed -i "s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"
fi
echo "Updated $CRATE version to $NEW_VERSION in $CARGO_TOML"

TAG="${CRATE}_v${NEW_VERSION}"
echo "Will create tag: $TAG"

confirmed=0

confirm() {
        read -p "$1 (y/N): " confirm
        if [[ ! "$confirm" =~ ^[Yy] ]]; then
                echo "Aborting."
                confirmed=0
        else
                confirmed=1
        fi
}

undoVersionChange() {
        if [[ "$OSTYPE" == "darwin"* ]]; then
                sed -i '' "s/^version = \".*\"/version = \"$CURRENT_VERSION\"/" "$CARGO_TOML"
        else
                sed -i "s/^version = \".*\"/version = \"$CURRENT_VERSION\"/" "$CARGO_TOML"
        fi
        echo "Reverted $CRATE version back to $CURRENT_VERSION in $CARGO_TOML"
}

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
        undoVersionChange
        exit 1
fi

git tag -a "$TAG" -m "$(echo -e "$TAG_MESSAGE")"

confirm "Do you want to push this tag to the remote origin?"
if [ $confirmed -ne 1 ]; then
    git tag -d "$TAG"
    echo "Tag $TAG deleted locally. Aborting push."
    undoVersionChange
    exit 1
fi

git push origin "$TAG"
