#!/usr/bin/bash

set -eu

cargo build
cargo fmt --check
cargo clippy -- -D warnings

ROOT="$PWD"
TARGET="$ROOT/target/debug/codecrafters-git"
TESTDIR="$ROOT/test-$$"
cleanup() {
    cd "$ROOT"
    rm -rf "$TESTDIR"
}
setup() {
    echo "$1"
    mkdir "$TESTDIR"
    cd "$TESTDIR"
}
trap cleanup EXIT
cleanup

diff_cmd() {
    git "$@" > ref
    "$TARGET" "$@" > mine
    diff -a mine ref
}

assert_init() {
    (
        cd "$1"
        test -d .git
        test -d .git/objects
        test -d .git/refs
        grep -q '^ref: refs/heads/main' .git/HEAD
    )
}

setup "git init"
"$TARGET" init > mine
assert_init "."
rm -rf .git
git init > ref
diff mine ref
cleanup

setup "git init <path>"
"$TARGET" init foo/bar > mine
assert_init foo/bar
rm -rf foo
git init foo/bar > ref
diff mine ref
cleanup

setup "git init <non-utf8-path>"
NAME=$(printf "abc\x80def")
"$TARGET" init "$NAME" >/dev/null
assert_init "$NAME"
# Don't compare with git: it just skips the rogue byte while
# my program prints abc�def instead, which I like better.
cleanup

setup "git cat-file -p <blob>"
SMALLFILE="$ROOT/Cargo.toml"
"$TARGET" init >/dev/null
BLOB=$(git hash-object -w "$SMALLFILE")
diff_cmd cat-file -p "$BLOB"
cleanup

setup "git cat-file -p <blob-with-non-utf8-content>"
printf "abc\x80def" > somefile
"$TARGET" init >/dev/null
BLOB=$(git hash-object -w somefile)
diff_cmd cat-file -p "$BLOB"
cleanup
