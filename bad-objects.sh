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
    TITLE=$1
    OBJECT=$2

    printf "\n=== $TITLE ===\n"
    printf "$OBJECT" | pigz -z > "$FILE"
    "$TARGET" cat-file -p $NAME || true
    printf "\n"
}

check "Good for reference" "blob 6\0hello\n"

check "Size: expected > got" "blob 7\0hello\n"
check "Size: expected < got" "blob 5\0hello\n"

check "Unknown type" "nope 6\0hello\n"

check "Non-numeric size" "blob abc\0hello\n"
check "Non-utf8 size" "blob 1\7772\0hello\n"

rm -rf "$DIR"

