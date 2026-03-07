#!/bin/bash

# upload-test-data.sh
#
# A script to populate your Opake vault with the sample data from test-data/.
# Use this to quickly see how Opake handles nested structures and different
# file types.
#
# NOTE: Currently, the Opake CLI's `mkdir` and `upload --dir` commands have
# limited support for nested paths. This script works best with single-level
# directories.

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

# Check if logged in by checking for a DID in the accounts list
if ! $OPAKE_BIN accounts | grep -q "did:plc"; then
    echo -e "${YELLOW}No accounts found.${RESET} Please run 'opake login' first."
    exit 1
fi

log "Starting test data upload..."

# --- Create Top-Level Directories ---

# Find first-level directories in test-data/
find "$TEST_DATA_DIR" -maxdepth 1 -type d -not -path "$TEST_DATA_DIR" | while read -r dir_path; do
    rel_dir=$(basename "$dir_path")
    log "Ensuring directory exists: /$rel_dir"
    # mkdir might fail if it exists, so we ignore errors here
    $OPAKE_BIN mkdir "$rel_dir" 2>/dev/null || warn "Directory '$rel_dir' might already exist."
done

# --- Upload Files ---

# Find all files in test-data/ and upload them
# We use -mindepth 1 to avoid the test-data directory itself
find "$TEST_DATA_DIR" -type f | while read -r file_path; do
    # Get relative path within test-data/
    rel_path=${file_path#$TEST_DATA_DIR/}
    
    # Extract filename and its parent directory
    filename=$(basename "$file_path")
    parent_dir=$(dirname "$rel_path")
    
    # If the file is in a nested directory (e.g., notes/anarchy-and-praxis),
    # we currently upload it to the top-level parent because the CLI
    # doesn't support recursive mkdir or nested --dir resolution well yet.
    top_level_parent=$(echo "$parent_dir" | cut -d'/' -f1)

    if [ "$parent_dir" == "." ]; then
        log "Uploading $filename to root..."
        $OPAKE_BIN upload "$file_path"
    else
        log "Uploading $filename to /$top_level_parent..."
        # We use the top_level_parent to ensure it goes into an existing folder
        $OPAKE_BIN upload "$file_path" --dir "$top_level_parent"
    fi
    
    success "Uploaded $filename"
done

echo -e "\n${GREEN}${BOLD}All test data uploaded! ✨${RESET}"
echo -e "Try running ${BOLD}opake tree${RESET} to see your new files."
