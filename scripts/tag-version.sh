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

UPDATED_DEPENDENTS=()
# Scan all other crates for direct dependencies on the bumped crate and bump their patch version
# POSIX-compatible dependency check and version bump
for dep_dir in "$CRATES_DIR"/*/; do
    dep_dir="${dep_dir%/}"
    dep_toml="$dep_dir/Cargo.toml"
    [ "$dep_toml" = "$CARGO_TOML" ] && continue
    [ ! -f "$dep_toml" ] && continue
    # Check for dependency on the bumped crate (workspace or path)
    if grep "^$CRATE[[:space:]]*=[[:space:]]*{[[:space:]]*workspace[[:space:]]*=[[:space:]]*true" "$dep_toml" >/dev/null 2>&1 || \
       grep "^$CRATE[[:space:]]*=[[:space:]]*{[[:space:]]*path[[:space:]]*=" "$dep_toml" >/dev/null 2>&1; then
        # Extract current version
        dep_version=$(grep '^version' "$dep_toml" | head -n1 | sed -E 's/version *= *"([0-9]+\.[0-9]+\.[0-9]+)"/\1/')
        dep_major=$(echo "$dep_version" | awk -F. '{print $1}')
        dep_minor=$(echo "$dep_version" | awk -F. '{print $2}')
        dep_patch=$(echo "$dep_version" | awk -F. '{print $3}')
        dep_patch=$((dep_patch + 1))
        dep_new_version="$dep_major.$dep_minor.$dep_patch"
        # Update version
        if [[ "$OSTYPE" == "darwin"* ]]; then
            sed -i '' "s/^version = \".*\"/version = \"$dep_new_version\"/" "$dep_toml"
        else
            sed -i "s/^version = \".*\"/version = \"$dep_new_version\"/" "$dep_toml"
        fi
        UPDATED_DEPENDENTS+=("$(basename "$dep_dir"): $dep_version → $dep_new_version")
        echo "Bumped patch version of $(basename "$dep_dir") to $dep_new_version (due to dependency on $CRATE)"
    fi
done


TAG="${CRATE}-v${NEW_VERSION}"
echo "Will create tag: $TAG"

# Tag the main crate
git tag "$TAG"
TAGS_CREATED=($TAG)

# Tag all updated dependents
if [ ${#UPDATED_DEPENDENTS[@]} -gt 0 ]; then
    echo "\nDependents updated:"
    for dep in "${UPDATED_DEPENDENTS[@]}"; do
        echo "  $dep"
        dep_dir_name=$(echo "$dep" | cut -d: -f1)
        dep_new_version=$(echo "$dep" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+$')
        # Find the full package name for this directory
        dep_pkg_name=""
        for i in "${!DIR_ARR[@]}"; do
            if [ "${DIR_ARR[$i]}" = "$CRATES_DIR/$dep_dir_name" ]; then
                dep_pkg_name="${PKG_ARR[$i]}"
                break
            fi
        done
        # Fallback to directory name if not found
        [ -z "$dep_pkg_name" ] && dep_pkg_name="$dep_dir_name"
        dep_tag="${dep_pkg_name}-v${dep_new_version}"
        dep_toml="$CRATES_DIR/$dep_dir_name/Cargo.toml"
        if [ -f "$dep_toml" ]; then
            git tag "$dep_tag"
            TAGS_CREATED+=("$dep_tag")
            echo "Tagged $dep_pkg_name as $dep_tag"
        fi
    done
fi

echo
echo "The following tags were created:"
for t in "${TAGS_CREATED[@]}"; do
    echo "  $t"
done
echo
read -p "Push all tags to the remote origin? (y/N): " confirm_push
if [[ "$confirm_push" =~ ^[Yy] ]]; then
    for t in "${TAGS_CREATED[@]}"; do
        git push origin "$t"
    done
    echo "All tags pushed."
else
    echo "Tags were created locally but not pushed."
fi
