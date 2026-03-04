#!/usr/bin/env bash
# =============================================================================
# LogSleuth release automation script (Unix / macOS / Linux)
#
# Validates and bumps the semantic version, updates Cargo.toml and Cargo.lock,
# runs a release build and full test suite, commits the version bump, creates
# an annotated git tag, pushes to origin, and prunes older release tags.
#
# Usage:
#   Interactive:       ./update-application.sh
#   Parameterised:     ./update-application.sh --version 1.2.0 --notes "Fix bug"
#   Force re-release:  ./update-application.sh --version 1.2.0 --notes "Rebuild" --force
#   Dry run:           ./update-application.sh --version 1.2.0 --notes "Preview" --dry-run
# =============================================================================
set -euo pipefail

# ---------------------------------------------------------------------------
# Coloured output helpers
# ---------------------------------------------------------------------------
info()    { printf "\033[36m[INFO]\033[0m %s\n" "$*"; }
success() { printf "\033[32m[OK]  \033[0m %s\n" "$*"; }
warn()    { printf "\033[33m[WARN]\033[0m %s\n" "$*"; }
err()     { printf "\033[31m[ERR] \033[0m %s\n" "$*"; }

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
VERSION=""
NOTES=""
FORCE=false
DRY_RUN=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version|-v)  VERSION="$2";  shift 2 ;;
        --notes|-n)    NOTES="$2";    shift 2 ;;
        --force|-f)    FORCE=true;    shift   ;;
        --dry-run|-d)  DRY_RUN=true;  shift   ;;
        *)
            err "Unknown argument: $1"
            echo "Usage: $0 [--version x.y.z] [--notes \"...\"] [--force] [--dry-run]"
            exit 1
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Resolve workspace root (directory containing this script)
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

CARGO_TOML="$SCRIPT_DIR/Cargo.toml"
LOCK_FILE="$SCRIPT_DIR/Cargo.lock"

# ---------------------------------------------------------------------------
# Version helpers
# ---------------------------------------------------------------------------
get_package_version() {
    grep -A5 '^\[package\]' "$CARGO_TOML" \
        | grep '^version' \
        | head -1 \
        | sed 's/.*"\([0-9]*\.[0-9]*\.[0-9]*\)".*/\1/'
}

compare_semver() {
    local IFS='.'
    local -a left=($1) right=($2)
    for i in 0 1 2; do
        if (( left[i] > right[i] )); then echo 1; return; fi
        if (( left[i] < right[i] )); then echo -1; return; fi
    done
    echo 0
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
echo ""
echo "  LogSleuth Release Automation (Unix)"
echo "  ===================================="
echo ""

if $DRY_RUN; then
    warn "Dry-run mode enabled. No files, commits, tags, pushes, or releases will be changed."
    echo ""
fi

CURRENT_VERSION="$(get_package_version)"
echo "Current version: $CURRENT_VERSION"
echo ""

# --- Collect version ---
if [[ -z "$VERSION" ]]; then
    read -rp "Enter new semantic version (x.y.z): " VERSION
fi

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    err "Version must match semantic versioning format x.y.z"
    exit 1
fi

if ! $FORCE; then
    CMP="$(compare_semver "$VERSION" "$CURRENT_VERSION")"
    if (( CMP <= 0 )); then
        err "New version ($VERSION) must be greater than current version ($CURRENT_VERSION). Use --force to override."
        exit 1
    fi
fi

# --- Collect release notes ---
if [[ -z "$NOTES" ]]; then
    echo "Enter release notes (end with an empty line):"
    NOTES=""
    while IFS= read -r line; do
        [[ -z "$line" ]] && break
        NOTES="${NOTES}${NOTES:+$'\n'}${line}"
    done
fi

if [[ -z "$NOTES" ]]; then
    err "Release notes are required and cannot be empty"
    exit 1
fi

# --- Git state checks ---
IS_GIT=false
if git rev-parse --is-inside-work-tree &>/dev/null; then
    IS_GIT=true
fi

if ! $IS_GIT && ! $DRY_RUN; then
    err "This script must be run inside a git repository."
    exit 1
fi

NEW_TAG="v$VERSION"

if $IS_GIT; then
    EXISTING_TAG="$(git tag -l "$NEW_TAG")"
    if [[ -n "$EXISTING_TAG" ]] && ! $FORCE; then
        err "Tag $NEW_TAG already exists. Use --force to overwrite."
        exit 1
    fi

    STATUS="$(git status --porcelain)"
    if [[ -n "$STATUS" ]]; then
        warn "Working tree has uncommitted changes."
    fi
fi

# --- Dry-run summary ---
if $DRY_RUN; then
    echo ""
    echo "Release Summary (Dry Run)"
    echo "-------------------------"
    echo "Current version : $CURRENT_VERSION"
    echo "New version     : $VERSION"
    echo "Tag             : $NEW_TAG"
    echo "Release notes   : $NOTES"
    echo ""
    info "Planned actions:"
    echo "  - Update [package] version in Cargo.toml: $CURRENT_VERSION -> $VERSION"
    echo "  - Run: cargo update"
    echo "  - Run: cargo build --release"
    echo "  - Run: cargo fmt -- --check"
    echo "  - Run: cargo clippy -- -D warnings"
    echo "  - Run: cargo test"
    if $IS_GIT; then
        echo "  - Run: git add Cargo.toml Cargo.lock"
        echo "  - Run: git commit -m 'chore: bump version to $VERSION'"
        echo "  - Run: git tag -a $NEW_TAG -m <notes>"
        echo "  - Run: git push origin HEAD"
        echo "  - Run: git push origin $NEW_TAG"
        echo "  - Prune older v*.*.* tags/releases (keep only $NEW_TAG)"
    fi
    echo ""
    success "Dry-run complete. No changes made."
    exit 0
fi

# --- Snapshot originals for rollback ---
ORIGINAL_CARGO="$(cat "$CARGO_TOML")"
ORIGINAL_LOCK=""
LOCK_EXISTED=false
if [[ -f "$LOCK_FILE" ]]; then
    LOCK_EXISTED=true
    ORIGINAL_LOCK="$(cat "$LOCK_FILE")"
fi

rollback() {
    warn "Rolling back version file changes"
    echo "$ORIGINAL_CARGO" > "$CARGO_TOML"
    if $LOCK_EXISTED && [[ -n "$ORIGINAL_LOCK" ]]; then
        echo "$ORIGINAL_LOCK" > "$LOCK_FILE"
    elif ! $LOCK_EXISTED && [[ -f "$LOCK_FILE" ]]; then
        rm -f "$LOCK_FILE"
    fi
    warn "Rollback completed"
}

trap 'rollback' ERR

# --- Step 1: Update version ---
if [[ "$VERSION" != "$CURRENT_VERSION" ]]; then
    info "Updating [package] version in Cargo.toml: $CURRENT_VERSION -> $VERSION"
    sed -i.bak "s/^version = \"$CURRENT_VERSION\"/version = \"$VERSION\"/" "$CARGO_TOML"
    rm -f "$CARGO_TOML.bak"

    info "Refreshing Cargo.lock via cargo update"
    cargo update
fi

# --- Step 2: Confirmation ---
echo ""
echo "Release Summary"
echo "---------------"
echo "Current version : $CURRENT_VERSION"
echo "New version     : $VERSION"
echo "Tag             : $NEW_TAG"
echo "Release notes   : $NOTES"
echo ""
read -rp "Proceed with build / test / release steps? (y/N) " CONFIRM
if [[ "$CONFIRM" != "y" && "$CONFIRM" != "Y" ]]; then
    err "Release cancelled by user"
    rollback
    exit 1
fi

# --- Step 3: Build ---
info "Running release build"
cargo build --release

# --- Step 4: Format check, Clippy, tests ---
info "Checking formatting (cargo fmt -- --check)"
cargo fmt -- --check

info "Running Clippy lints (cargo clippy -- -D warnings)"
cargo clippy -- -D warnings

info "Running full test suite"
cargo test

# --- Step 5: Handle existing tag (Force scenario) ---
if $FORCE && $IS_GIT; then
    EXISTING_TAG="$(git tag -l "$NEW_TAG")"
    if [[ -n "$EXISTING_TAG" ]]; then
        warn "Removing existing local tag $NEW_TAG (--force)"
        git tag -d "$NEW_TAG"
        if git ls-remote --tags origin "$NEW_TAG" | grep -q "$NEW_TAG"; then
            warn "Removing existing remote tag $NEW_TAG (--force)"
            git push origin --delete "$NEW_TAG"
        fi
    fi
fi

# --- Step 6: Commit version bump ---
if $IS_GIT && [[ "$VERSION" != "$CURRENT_VERSION" ]]; then
    info "Staging and committing version bump"
    git add Cargo.toml
    [[ -f "$LOCK_FILE" ]] && git add Cargo.lock
    git commit -m "chore: bump version to $VERSION"
fi

# --- Step 7: Tag and push ---
if $IS_GIT; then
    info "Creating annotated tag $NEW_TAG"
    git tag -a "$NEW_TAG" -m "$NOTES"

    info "Pushing commit and tag to origin"
    git push origin HEAD
    git push origin "$NEW_TAG"
fi

# --- Step 8: Prune older tags ---
if $IS_GIT; then
    info "Cleaning up older release tags (keeping $NEW_TAG)"
    for old_tag in $(git tag -l 'v*.*.*'); do
        [[ "$old_tag" == "$NEW_TAG" ]] && continue
        [[ -z "$old_tag" ]] && continue

        info "  Removing local tag $old_tag"
        git tag -d "$old_tag" || true

        if git ls-remote --tags origin "$old_tag" 2>/dev/null | grep -q "$old_tag"; then
            info "  Removing remote tag $old_tag"
            git push origin --delete "$old_tag" || true
        fi

        if command -v gh &>/dev/null; then
            gh release delete "$old_tag" --yes 2>/dev/null || true
        fi
    done
fi

# --- Done ---
echo ""
success "Release $NEW_TAG submitted."
success "CI will build platform binaries and publish the GitHub Release."
if $IS_GIT; then
    REMOTE_URL="$(git config --get remote.origin.url 2>/dev/null || true)"
    if [[ -n "$REMOTE_URL" ]]; then
        # Convert git@github.com:... to https://...
        HTTPS_URL="$(echo "$REMOTE_URL" | sed -E 's#^git@github\.com:(.+?)(\.git)?$#https://github.com/\1#')"
        HTTPS_URL="${HTTPS_URL%.git}"
        success "Monitor pipeline: $HTTPS_URL/actions"
    fi
fi
echo ""

# Clear the ERR trap on successful completion
trap - ERR
