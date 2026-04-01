#!/bin/bash

# upload-workspace-test-data.sh
#
# Create a shared workspace and populate it with test data.
# Requires three signed-in accounts:
#   1. Default account (owner/manager)
#   2. An editor   — pass handle via $EDITOR_HANDLE (default: annoiiyed.bsky.social)
#   3. A viewer    — pass handle via $VIEWER_HANDLE (default: viewer.test)
#
# Usage:
#   ./tools/upload-workspace-test-data.sh
#   EDITOR_HANDLE=bob.bsky.social VIEWER_HANDLE=carol.bsky.social ./tools/upload-workspace-test-data.sh

set -e

# --- Configuration ---
OPAKE_BIN="cargo run --quiet --package opake-cli --"
TEST_DATA_DIR="test-data/workspace"
WORKSPACE_NAME="Cryptid Field Research"
EDITOR_HANDLE="${EDITOR_HANDLE:-annoiiyed.bsky.social}"
VIEWER_HANDLE="${VIEWER_HANDLE:-viewer.test}"

# Colors
BOLD="\033[1m"
CYAN="\033[36m"
GREEN="\033[32m"
YELLOW="\033[33m"
RESET="\033[0m"

log() { echo -e "${CYAN}==>${RESET} ${BOLD}$1${RESET}"; }
success() { echo -e "${GREEN}✓${RESET} $1"; }
warn() { echo -e "${YELLOW}!${RESET} $1"; }

# --- Check Prerequisites ---

if ! [ -d "$TEST_DATA_DIR" ]; then
    echo "Error: $TEST_DATA_DIR directory not found. Run this from the project root."
    exit 1
fi

if ! $OPAKE_BIN account list | grep -q "did:plc"; then
    echo -e "${YELLOW}No accounts found.${RESET} Please run 'opake account login' first."
    exit 1
fi

# --- Create Workspace ---

log "Creating workspace: $WORKSPACE_NAME"
$OPAKE_BIN workspace create "$WORKSPACE_NAME" 2>/dev/null \
    && success "Workspace created" \
    || warn "Workspace might already exist"

# --- Add Members ---

log "Adding editor: $EDITOR_HANDLE"
$OPAKE_BIN workspace add-member "$WORKSPACE_NAME" "$EDITOR_HANDLE" --role editor 2>/dev/null \
    && success "$EDITOR_HANDLE → editor" \
    || warn "$EDITOR_HANDLE (failed — already a member or account not found)"

log "Adding viewer: $VIEWER_HANDLE"
$OPAKE_BIN workspace add-member "$WORKSPACE_NAME" "$VIEWER_HANDLE" --role viewer 2>/dev/null \
    && success "$VIEWER_HANDLE → viewer" \
    || warn "$VIEWER_HANDLE (failed — already a member or account not found)"

# --- Create Directories ---

log "Creating directories..."

for dir_name in field-guides sketches journal; do
    $OPAKE_BIN mkdir "$dir_name" --workspace "$WORKSPACE_NAME" 2>/dev/null \
        && success "/$dir_name" \
        || warn "/$dir_name might already exist"
done

# --- Upload Files ---

log "Uploading files..."

find "$TEST_DATA_DIR" -type f -not -name '.*' -print0 | while IFS= read -r -d '' file_path; do
    rel_path=${file_path#$TEST_DATA_DIR/}
    filename=$(basename "$file_path")
    parent_dir=$(dirname "$rel_path")

    if [ "$parent_dir" = "." ]; then
        $OPAKE_BIN upload "$file_path" --workspace "$WORKSPACE_NAME" \
            && success "$filename → /" \
            || warn "$filename (failed)"
    else
        $OPAKE_BIN upload "$file_path" --workspace "$WORKSPACE_NAME" --dir "$parent_dir" \
            && success "$filename → /$parent_dir" \
            || warn "$filename (failed)"
    fi
done

# --- Tag Files ---

log "Tagging files..."

tag() {
    local doc="$1" tag="$2"
    $OPAKE_BIN metadata tag add "$doc" "$tag" --workspace "$WORKSPACE_NAME" 2>/dev/null \
        && success "$doc +$tag" \
        || warn "$doc +$tag (failed)"
}

# Field guides
for f in field-guides/mothman.md field-guides/jackalope.md field-guides/flatwoods-monster.md field-guides/witte-wieven.md; do
    tag "$f" "field-guide"
    tag "$f" "cryptids"
done

# Sketches
for f in sketches/mothman-sketch.png sketches/jackalope-postcard.jpg sketches/flatwoods-monster.png sketches/witte-wieven.jpg; do
    tag "$f" "sketches"
    tag "$f" "images"
done

# Journal
tag "journal/expedition-log.md" "journal"
tag "journal/expedition-log.md" "field-work"
tag "journal/equipment-checklist.md" "logistics"

# --- Done ---

echo -e "\n${GREEN}${BOLD}Workspace populated!${RESET}"
echo -e "Owner (manager):  default account"
echo -e "Editor:           $EDITOR_HANDLE"
echo -e "Viewer:           $VIEWER_HANDLE"
echo -e "\nTry: ${BOLD}opake workspace ls -l${RESET}"
