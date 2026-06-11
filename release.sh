#!/bin/bash

# Suppress command output unless there is an error
run_cmd() {
    local cmd_name="$1"
    shift
    
    local output
    output=$( "$@" 2>&1 )
    local status=$?
    
    if [ $status -ne 0 ]; then
        echo "❌ $cmd_name FAILED (exit code $status):"
        echo "$output"
        exit $status
    fi
}

bump_version() {
    local bump_type="$1"
    local current="$2"
    
    if [[ "$bump_type" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "$bump_type"
        return
    fi
    
    local major=$(echo "$current" | cut -d. -f1)
    local minor=$(echo "$current" | cut -d. -f2)
    local patch=$(echo "$current" | cut -d. -f3)
    
    case "$bump_type" in
        major)
            major=$((major + 1))
            minor=0
            patch=0
            ;;
        minor)
            minor=$((minor + 1))
            patch=0
            ;;
        patch)
            patch=$((patch + 1))
            ;;
        *)
            echo "Error: Unknown bump type: $bump_type" >&2
            return 1
            ;;
    esac
    echo "$major.$minor.$patch"
}

# Parse Cargo.toml package version
get_current_version() {
    grep -m1 '^version =' Cargo.toml | cut -d'"' -f2
}

# Set default values
BUMP="patch"
COMMIT_MSG=""
PUSH=true
TAG=true
YES=false

# CLI Help
show_help() {
    echo "Usage: ./release.sh [options]"
    echo ""
    echo "Options:"
    echo "  -b, --bump TYPE      Bump type: patch (default), minor, major, none, or explicit version (e.g. 0.1.28)"
    echo "  -m, --message MSG    Commit message. Defaults to 'bump version to x.y.z'"
    echo "  --no-push            Do not push commits or tags to remote"
    echo "  --no-tag             Do not create a git tag"
    echo "  -y, --yes            Skip confirmation prompt"
    echo "  -h, --help           Show this help"
    exit 0
}

# Parse options
while [[ $# -gt 0 ]]; do
    case "$1" in
        -b|--bump)
            BUMP="$2"
            shift 2
            ;;
        -m|--message)
            COMMIT_MSG="$2"
            shift 2
            ;;
        --no-push)
            PUSH=false
            shift
            ;;
        --no-tag)
            TAG=false
            shift
            ;;
        -y|--yes)
            YES=true
            shift
            ;;
        -h|--help)
            show_help
            ;;
        *)
            echo "Unknown option: $1"
            show_help
            ;;
    esac
done

# Check current version
CURRENT_VERSION=$(get_current_version)
if [ -z "$CURRENT_VERSION" ]; then
    echo "❌ Error: Could not find package version in Cargo.toml"
    exit 1
fi

# Calculate new version
NEW_VERSION="$CURRENT_VERSION"
if [ "$BUMP" != "none" ]; then
    NEW_VERSION=$(bump_version "$BUMP" "$CURRENT_VERSION")
    if [ $? -ne 0 ]; then
        exit 1
    fi
fi

# Set default commit message if none provided
if [ -z "$COMMIT_MSG" ]; then
    if [ "$BUMP" = "none" ]; then
        COMMIT_MSG="release version $CURRENT_VERSION"
    else
        COMMIT_MSG="bump version to $NEW_VERSION"
    fi
fi

# Summary of actions
echo "========================================"
echo "Proposed Release Plan:"
echo "  Current Version:  $CURRENT_VERSION"
echo "  New Version:      $NEW_VERSION (bump: $BUMP)"
echo "  Commit Message:   $COMMIT_MSG"
echo "  Create Tag:       $( [ "$TAG" = true ] && echo "Yes (v$NEW_VERSION)" || echo "No" )"
echo "  Push to Remote:   $( [ "$PUSH" = true ] && echo "Yes" || echo "No" )"
echo "========================================"

if [ "$YES" = false ]; then
    read -p "Proceed? (y/N): " confirm
    if [[ ! "$confirm" =~ ^[yY](es)?$ ]]; then
        echo "Release aborted."
        exit 0
    fi
fi

# 1. Run checks and tests first
echo "Running pre-release tests and quality checks..."
run_cmd "Quality checks" ./check_all.sh
run_cmd "Automated tests" ./run_tests.sh

# 2. Bump version in Cargo.toml
if [ "$BUMP" != "none" ]; then
    echo "Updating Cargo.toml version to $NEW_VERSION..."
    sed -i 's/^version = .*/version = "'"$NEW_VERSION"'"/' Cargo.toml
fi

# 3. Git Stage & Commit
echo "Creating git commit..."
# Add modified/untracked files (keeping Cargo.toml, but user can stage others beforehand)
git add Cargo.toml
run_cmd "Git commit" git commit -m "$COMMIT_MSG"

# 4. Git Tag
if [ "$TAG" = true ]; then
    echo "Creating git tag v$NEW_VERSION..."
    run_cmd "Git tag" git tag "v$NEW_VERSION"
fi

# 5. Git Push
if [ "$PUSH" = true ]; then
    echo "Pushing commits to remote..."
    run_cmd "Git push" git push
    if [ "$TAG" = true ]; then
        echo "Pushing tags to remote..."
        run_cmd "Git push tags" git push --tags
    fi
fi

echo "🎉 Release completed successfully!"
