#!/bin/bash

# upload-test-data.sh
#
# Populate your Opake vault with the sample data from test-data/.
# Creates the full nested directory structure, then uploads all files
# into their correct locations.

set -e

# --- Configuration ---
OPAKE_BIN="cargo run --quiet --package opake-cli --"
TEST_DATA_DIR="test-data"

# Colors for dopamine
BOLD="\033[1m"
CYAN="\033[36m"
GREEN="\033[32m"
YELLOW="\033[33m"
RESET="\033[0m"

# --- Functions ---

log() {
    echo -e "${CYAN}==>${RESET} ${BOLD}$1${RESET}"
}

success() {
    echo -e "${GREEN}✓${RESET} $1"
}

warn() {
    echo -e "${YELLOW}!${RESET} $1"
}

# --- Check Prerequisites ---

if ! [ -d "$TEST_DATA_DIR" ]; then
    echo "Error: $TEST_DATA_DIR directory not found. Please run this from the project root."
    exit 1
fi

if ! $OPAKE_BIN account list | grep -q "did:plc"; then
    echo -e "${YELLOW}No accounts found.${RESET} Please run 'opake login' first."
    exit 1
fi

log "Starting test data upload..."

# --- Create Directories (depth-first, parents before children) ---

# Sort by depth so parent directories are created before their children.
find "$TEST_DATA_DIR" -type d -not -path "$TEST_DATA_DIR" | awk -F/ '{print NF, $0}' | sort -n | cut -d' ' -f2- | while read -r dir_path; do
    rel_path=${dir_path#$TEST_DATA_DIR/}
    dir_name=$(basename "$rel_path")
    parent_dir=$(dirname "$rel_path")

    if [ "$parent_dir" = "." ]; then
        log "Creating directory: /$dir_name"
        $OPAKE_BIN mkdir "$dir_name" 2>/dev/null || warn "Directory '$dir_name' might already exist."
    else
        log "Creating directory: /$rel_path"
        $OPAKE_BIN mkdir "$dir_name" --dir "$parent_dir" 2>/dev/null || warn "Directory '$rel_path' might already exist."
    fi
done

# --- Upload Files (sequential — PDS uses repo-level optimistic locking) ---

log "Uploading files..."

find "$TEST_DATA_DIR" -type f -print0 | while IFS= read -r -d '' file_path; do
    rel_path=${file_path#$TEST_DATA_DIR/}
    filename=$(basename "$file_path")
    parent_dir=$(dirname "$rel_path")

    if [ "$parent_dir" = "." ]; then
        $OPAKE_BIN upload "$file_path" && \
            success "$filename → /" || \
            warn "$filename (failed)"
    else
        $OPAKE_BIN upload "$file_path" --dir "$parent_dir" && \
            success "$filename → /$parent_dir" || \
            warn "$filename (failed)"
    fi
done

# --- Tag Files ---

log "Tagging files..."

tag() {
    local doc="$1" tag="$2"
    $OPAKE_BIN metadata tag add "$doc" "$tag" 2>/dev/null && \
        success "$doc +$tag" || \
        warn "$doc +$tag (failed)"
}

# Grocery lists
tag "grocery-lists/week-10.md" "groceries"
tag "grocery-lists/week-10.md" "lists"
tag "grocery-lists/week-11.md" "groceries"
tag "grocery-lists/week-11.md" "lists"

# Anarchy & praxis
for f in notes/anarchy-and-praxis/anti-luddism.md notes/anarchy-and-praxis/manifesto.md notes/anarchy-and-praxis/praxis-todo.md; do
    tag "$f" "politics"
    tag "$f" "theory"
done

# Fantasy creatures — text files
for creature_dir in dragon griffin phoenix roly-poly; do
    find "$TEST_DATA_DIR/notes/fantasy-creatures/$creature_dir" -name "*.md" -print0 | while IFS= read -r -d '' f; do
        rel=${f#$TEST_DATA_DIR/}
        tag "$rel" "creatures"
        tag "$rel" "worldbuilding"
    done
done

# Fantasy creatures — images (roly-poly)
find "$TEST_DATA_DIR/notes/fantasy-creatures/roly-poly" \( -name "*.jpg" -o -name "*.JPG" -o -name "*.png" \) -print0 | while IFS= read -r -d '' f; do
    rel=${f#$TEST_DATA_DIR/}
    tag "$rel" "creatures"
    tag "$rel" "images"
done

# Top-level notes
for f in notes/architecture-ideas.md notes/css-todo.md notes/queer-theory-reading-list.md notes/todo-list.md; do
    tag "$f" "notes"
done

# Poetry
for f in poetry/null-pointer.md poetry/quinn-swoop.md poetry/roly-poly.md poetry/the-void.md; do
    tag "$f" "poetry"
    tag "$f" "writing"
done

echo -e "\n${GREEN}${BOLD}All test data uploaded and tagged!${RESET}"
echo -e "Try running ${BOLD}opake tree${RESET} to see your new files."
