#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: ./release.sh [options]

Create a release commit and an annotated version tag from the Unreleased
changelog section. Run this only after the feature changes and changelog entry
have already been committed.

Options:
  -b, --bump TYPE      patch (default), minor, major, or an explicit x.y.z
  -m, --message MSG    Commit message (default: release vX.Y.Z)
      --no-push        Create the commit and tag locally without pushing
      --no-tag         Do not create a tag (also suppresses the tag push)
      --dry-run        Show the validated release plan without changing files
  -y, --yes            Skip the confirmation prompt
  -h, --help           Show this help
EOF
}

die() {
    echo "Error: $*" >&2
    exit 1
}

current_version() {
    awk '
        /^\[package\]$/ { in_package = 1; next }
        /^\[/ && in_package { exit }
        in_package && /^version = "/ {
            gsub(/^version = "|"$/, "")
            print
            exit
        }
    ' Cargo.toml
}

bump_version() {
    local bump=$1
    local current=$2
    local major minor patch

    if [[ $bump =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        printf '%s\n' "$bump"
        return
    fi

    IFS=. read -r major minor patch <<<"$current"
    case $bump in
        major) printf '%d.0.0\n' "$((major + 1))" ;;
        minor) printf '%d.%d.0\n' "$major" "$((minor + 1))" ;;
        patch) printf '%d.%d.%d\n' "$major" "$minor" "$((patch + 1))" ;;
        *) die "unknown bump type '$bump'" ;;
    esac
}

replace_package_version() {
    local file=$1
    local old=$2
    local new=$3
    local temporary=$4

    awk -v old="$old" -v new="$new" '
        /^\[package\]$/ { in_package = 1 }
        in_package && $0 == "version = \"" old "\"" && !replaced {
            print "version = \"" new "\""
            replaced = 1
            next
        }
        { print }
        END { if (!replaced) exit 2 }
    ' "$file" > "$temporary" || die "could not update the package version in $file"
    mv "$temporary" "$file"
}

replace_lockfile_version() {
    local old=$1
    local new=$2
    local temporary=$3

    awk -v old="$old" -v new="$new" '
        /^\[\[package\]\]$/ { package_block = 1; is_root = 0 }
        package_block && $0 == "name = \"rl-platform-overlay\"" { is_root = 1 }
        is_root && $0 == "version = \"" old "\"" && !replaced {
            print "version = \"" new "\""
            replaced = 1
            next
        }
        { print }
        END { if (!replaced) exit 2 }
    ' Cargo.lock > "$temporary" || die "could not update the package version in Cargo.lock"
    mv "$temporary" Cargo.lock
}

promote_changelog() {
    local version=$1
    local date=$2
    local temporary=$3

    awk -v version="$version" -v date="$date" '
        /^## \[Unreleased\]$/ {
            found = 1
            in_unreleased = 1
            print
            print ""
            print "---"
            print ""
            print "## [" version "] - " date
            next
        }
        in_unreleased && /^---$/ {
            in_unreleased = 0
            print "---"
            next
        }
        { print }
        END { if (!found || in_unreleased) exit 2 }
    ' CHANGELOG.md > "$temporary" || die "could not promote the Unreleased changelog section"
    mv "$temporary" CHANGELOG.md
}

bump=patch
message=
push=true
tag=true
confirm=true
dry_run=false

while (($#)); do
    case $1 in
        -b|--bump)
            (($# >= 2)) || die "$1 requires a value"
            bump=$2
            shift 2
            ;;
        -m|--message)
            (($# >= 2)) || die "$1 requires a value"
            message=$2
            shift 2
            ;;
        --no-push) push=false; shift ;;
        --no-tag) tag=false; shift ;;
        --dry-run) dry_run=true; shift ;;
        -y|--yes) confirm=false; shift ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown option '$1' (use --help for usage)" ;;
    esac
done

git rev-parse --is-inside-work-tree >/dev/null 2>&1 || die "run this script inside the repository"
[[ -z $(git status --porcelain) ]] || die "the working tree must be clean; commit the release changes first"

branch=$(git branch --show-current)
[[ -n $branch ]] || die "releases cannot be created from a detached HEAD"

old_version=$(current_version)
[[ $old_version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "Cargo.toml has no valid package version"
new_version=$(bump_version "$bump" "$old_version")
[[ $new_version != "$old_version" ]] || die "new version must differ from $old_version"

release_tag="v$new_version"
if $tag && git show-ref --verify --quiet "refs/tags/$release_tag"; then
    die "local tag $release_tag already exists"
fi

if $push; then
    git remote get-url origin >/dev/null 2>&1 || die "remote 'origin' is not configured"
    if $tag; then
        set +e
        git ls-remote --exit-code --tags origin "refs/tags/$release_tag" >/dev/null 2>&1
        remote_tag_status=$?
        set -e
        case $remote_tag_status in
            0) die "remote tag $release_tag already exists" ;;
            2) ;;
            *) die "could not check whether remote tag $release_tag exists" ;;
        esac
    fi
fi

unreleased=$(awk '
    /^## \[Unreleased\]$/ { found = 1; capture = 1; next }
    capture && /^---$/ { exit }
    capture { print }
    END { if (!found) exit 2 }
' CHANGELOG.md) || die "CHANGELOG.md has no valid Unreleased section"
grep -q '^### ' <<<"$unreleased" || die "the Unreleased changelog section needs at least one categorized entry"

[[ -n $message ]] || message="release $release_tag"

echo "Release plan"
echo "  Version: $old_version -> $new_version"
echo "  Branch:  $branch"
echo "  Commit:  $message"
echo "  Tag:     $($tag && echo "$release_tag" || echo "disabled")"
echo "  Push:    $($push && echo "origin/$branch" || echo "disabled")"

if $dry_run; then
    echo "Dry run complete; no files changed."
    exit 0
fi

if $confirm; then
    read -r -p "Proceed? [y/N] " answer
    [[ $answer =~ ^[Yy]([Ee][Ss])?$ ]] || { echo "Release aborted."; exit 0; }
fi

temporary_directory=$(mktemp -d)
trap 'rm -rf -- "$temporary_directory"' EXIT

replace_package_version Cargo.toml "$old_version" "$new_version" "$temporary_directory/Cargo.toml"
replace_lockfile_version "$old_version" "$new_version" "$temporary_directory/Cargo.lock"
promote_changelog "$new_version" "$(date +%F)" "$temporary_directory/CHANGELOG.md"

echo "Running the canonical release checks..."
./check_all.sh

git add -- Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "$message"

if $tag; then
    git tag -a "$release_tag" -m "$release_tag"
fi

if $push; then
    if $tag; then
        git push --atomic origin "$branch" "refs/tags/$release_tag"
    else
        git push origin "$branch"
    fi
fi

echo "Release $release_tag completed successfully."
