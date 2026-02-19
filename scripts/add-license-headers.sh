#!/bin/bash
# Script to add or update license headers in Rust source files
# Usage: ./scripts/add-license-headers.sh [--check]
#
# Options:
#   --check    Only check for missing headers, don't modify files (for CI)
#
# This script reads the license header from licenseheader.txt and ensures
# all .rs files in src/, examples/, conductor-macros/src/, and tests/ have it.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
LICENSE_FILE="$PROJECT_ROOT/licenseheader.txt"
CHECK_ONLY=false
MISSING_COUNT=0
UPDATED_COUNT=0

# Parse arguments
if [[ "$1" == "--check" ]]; then
    CHECK_ONLY=true
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if license file exists
if [[ ! -f "$LICENSE_FILE" ]]; then
    echo -e "${RED}Error: License header file not found: $LICENSE_FILE${NC}"
    exit 1
fi

# Read the license header
LICENSE_HEADER=$(cat "$LICENSE_FILE")
LICENSE_FIRST_LINE=$(head -n 1 "$LICENSE_FILE")

# Function to check if file has the license header
has_license_header() {
    local file="$1"
    local first_line=$(head -n 1 "$file")
    [[ "$first_line" == "$LICENSE_FIRST_LINE" ]]
}

# Function to add license header to a file
add_license_header() {
    local file="$1"
    local temp_file=$(mktemp)
    
    # Write license header followed by blank line and original content
    echo "$LICENSE_HEADER" > "$temp_file"
    echo "" >> "$temp_file"
    cat "$file" >> "$temp_file"
    
    # Replace original file
    mv "$temp_file" "$file"
}

# Function to update license header in a file (replace existing header)
update_license_header() {
    local file="$1"
    local temp_file=$(mktemp)
    local in_header=true
    local header_ended=false
    
    # Write new license header
    echo "$LICENSE_HEADER" > "$temp_file"
    
    # Skip old header lines (lines starting with // at the top)
    while IFS= read -r line; do
        if $in_header; then
            # Check if this line is part of the header (starts with // or is empty)
            if [[ "$line" =~ ^//.*$ ]]; then
                continue  # Skip old header line
            elif [[ -z "$line" ]] && ! $header_ended; then
                header_ended=true
                continue  # Skip empty line after header
            else
                in_header=false
                echo "" >> "$temp_file"  # Add blank line after new header
                echo "$line" >> "$temp_file"
            fi
        else
            echo "$line" >> "$temp_file"
        fi
    done < "$file"
    
    mv "$temp_file" "$file"
}

# Find all Rust files
find_rust_files() {
    {
        find "$PROJECT_ROOT/src" -name "*.rs" -type f 2>/dev/null
        find "$PROJECT_ROOT/examples" -name "*.rs" -type f 2>/dev/null
        find "$PROJECT_ROOT/conductor-macros/src" -name "*.rs" -type f 2>/dev/null
        find "$PROJECT_ROOT/tests" -name "*.rs" -type f 2>/dev/null
    } | sort
}

echo "License Header Tool"
echo "==================="
echo ""
echo "License header file: $LICENSE_FILE"
echo "Mode: $(if $CHECK_ONLY; then echo 'Check only'; else echo 'Add/Update'; fi)"
echo ""

# Process each file
while IFS= read -r file; do
    relative_path="${file#$PROJECT_ROOT/}"
    
    if has_license_header "$file"; then
        if ! $CHECK_ONLY; then
            echo -e "${GREEN}✓${NC} $relative_path"
        fi
    else
        ((MISSING_COUNT++))
        if $CHECK_ONLY; then
            echo -e "${RED}✗${NC} Missing header: $relative_path"
        else
            # Check if file starts with any comment (might be old header)
            first_char=$(head -c 2 "$file")
            if [[ "$first_char" == "//" ]]; then
                # Has some header, update it
                update_license_header "$file"
                echo -e "${YELLOW}↻${NC} Updated: $relative_path"
            else
                # No header at all, add it
                add_license_header "$file"
                echo -e "${GREEN}+${NC} Added: $relative_path"
            fi
            ((UPDATED_COUNT++))
        fi
    fi
done < <(find_rust_files)

echo ""
echo "==================="

if $CHECK_ONLY; then
    if [[ $MISSING_COUNT -gt 0 ]]; then
        echo -e "${RED}Found $MISSING_COUNT file(s) missing license headers${NC}"
        echo ""
        echo "Run './scripts/add-license-headers.sh' to add them."
        exit 1
    else
        echo -e "${GREEN}All files have license headers${NC}"
        exit 0
    fi
else
    if [[ $UPDATED_COUNT -gt 0 ]]; then
        echo -e "${GREEN}Updated $UPDATED_COUNT file(s)${NC}"
    else
        echo -e "${GREEN}All files already have license headers${NC}"
    fi
    exit 0
fi
