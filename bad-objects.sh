#!/bin/sh

# Test the program behaviour on bad objects, in a semi-automated way:
# program behaviour is not validated other than by visual inspection.

set -eu

# Use the xx sub-directory of .git/objects for our bad objects.
# Rely on the lack of validation of "hashes" / object names.
DIR='.git/objects/xx'
mkdir -p "$DIR"
FILE="$DIR/test"
NAME="xxtest"

cargo build
ROOT="$PWD"
TARGET="$ROOT/target/debug/codecrafters-git"

check () {
    if [ "x$1" = "x-t" ]; then
        CMD="ls-tree --name-only"
        shift
    else
        CMD="cat-file -p"
    fi
    TITLE=$1
    OBJECT=$2

    printf "\n=== $TITLE ===\n"
    printf "$OBJECT" | pigz -z > "$FILE"
    "$TARGET" $CMD "$NAME" || true
    printf "\n"
}

echo "*** Blobs ***"

check "Good for reference" "blob 6\0hello\n"

check "Size: expected > got" "blob 7\0hello\n"
check "Size: expected < got" "blob 5\0hello\n"

check "Unknown type" "nope 6\0hello\n"

check "Non-numeric size" "blob abc\0hello\n"
check "Non-utf8 size" "blob 1\7772\0hello\n"

echo "*** Trees ***"

check -t "Good (empty) for reference" "tree 0\0"
check -t "Good (single entry) for reference" "tree 30\0mode name\0hhhhhhhhhhhhhhhhhhhh"

check -t "Size: expected > got" "tree 1\0"
check -t "Size: expected < got" "tree 0\0 "

check -t "EOF while reading mode (no space)" "tree 6\000100644"
check -t "EOF while reading name (no nul byte)" "tree 10\000100644 abc"
check -t "EOF while reading hash (empty)" "tree 11\000100644 abc\0"
check -t "EOF while reading hash (19 bytes)" "tree 11\000100644 abc\0"
check -t "EOF while reading 2nd entry (single byte)"  "tree 31\0mode name\0hhhhhhhhhhhhhhhhhhhh "

rm -rf "$DIR"
