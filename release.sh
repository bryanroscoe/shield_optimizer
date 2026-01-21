#!/bin/bash
set -e

# Release script for Shield-Optimizer
# Creates a new GitHub release with semantic versioning (v0.0.0)

SCRIPT_FILE="Shield-Optimizer.ps1"
INITIAL_VERSION="0.69.0"

# Parse flags
MAJOR=false
MINOR=false

usage() {
    echo "Usage: $0 [--major | --minor]"
    echo ""
    echo "  --major    Bump major version (x.0.0)"
    echo "  --minor    Bump minor version (0.x.0)"
    echo "  (default)  Bump patch version (0.0.x)"
    echo ""
    echo "Examples:"
    echo "  $0           # v0.69.0 -> v0.69.1"
    echo "  $0 --minor   # v0.69.1 -> v0.70.0"
    echo "  $0 --major   # v0.70.0 -> v1.0.0"
    exit 1
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --major)
            MAJOR=true
            shift
            ;;
        --minor)
            MINOR=true
            shift
            ;;
        --help|-h)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

# Get the latest version tag
LATEST_TAG=$(git tag --list 'v*.*.*' --sort=-v:refname | head -1)

if [ -z "$LATEST_TAG" ]; then
    NEW_VERSION="v${INITIAL_VERSION}"
    echo "No existing tags found. Starting at $NEW_VERSION"
else
    # Parse semantic version (remove 'v' prefix)
    VERSION="${LATEST_TAG#v}"
    IFS='.' read -r MAJOR_NUM MINOR_NUM PATCH_NUM <<< "$VERSION"

    if $MAJOR; then
        MAJOR_NUM=$((MAJOR_NUM + 1))
        MINOR_NUM=0
        PATCH_NUM=0
        echo "Major release: $LATEST_TAG -> v${MAJOR_NUM}.${MINOR_NUM}.${PATCH_NUM}"
    elif $MINOR; then
        MINOR_NUM=$((MINOR_NUM + 1))
        PATCH_NUM=0
        echo "Minor release: $LATEST_TAG -> v${MAJOR_NUM}.${MINOR_NUM}.${PATCH_NUM}"
    else
        PATCH_NUM=$((PATCH_NUM + 1))
        echo "Patch release: $LATEST_TAG -> v${MAJOR_NUM}.${MINOR_NUM}.${PATCH_NUM}"
    fi

    NEW_VERSION="v${MAJOR_NUM}.${MINOR_NUM}.${PATCH_NUM}"
fi

# Confirm with user
echo ""
read -p "Create release $NEW_VERSION? (y/n) " -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 1
fi

# Check for uncommitted changes
if [ -n "$(git status --porcelain)" ]; then
    echo "Warning: You have uncommitted changes."
    read -p "Continue anyway? (y/n) " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted. Please commit your changes first."
        exit 1
    fi
fi

# Create and push tag
echo "Creating tag $NEW_VERSION..."
git tag -a "$NEW_VERSION" -m "Release $NEW_VERSION"
git push origin "$NEW_VERSION"

# Create GitHub release with the PowerShell script attached
echo "Creating GitHub release..."
gh release create "$NEW_VERSION" \
    --title "Shield Optimizer $NEW_VERSION" \
    --generate-notes \
    "$SCRIPT_FILE"

echo ""
echo "Release $NEW_VERSION created successfully!"
echo "View at: $(gh release view "$NEW_VERSION" --json url -q .url)"
