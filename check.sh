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
    git "$@" > /tmp/ref
    "$TARGET" "$@" >/tmp/mine
    diff -a /tmp/mine /tmp/ref
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

populate_tree() {
    # all types except submodule, in the top-level dir
    echo foo > afile
    touch empty-file
    mkdir dir
    touch dir/f # so that dir is not ignored
    ln -s dir/f link-rel
    ln -s "$PWD/dir/f" link-abs
    echo '#!/bin/true' > script
    chmod +x script
    # write-tree should ignore .git and empty directories
    mkdir -p ignored-dir/.git
    echo abc123 > ignored-dir/.git/HEAD
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

setup "git cat-file -p <blob> (from sub-directory)"
SMALLFILE="$ROOT/Cargo.toml"
"$TARGET" init >/dev/null
BLOB=$(git hash-object -w "$SMALLFILE")
mkdir -p foo/bar
cd foo/bar
diff_cmd cat-file -p "$BLOB"
cleanup

setup "git hash-object <file>"
SMALLFILE="$ROOT/Cargo.toml"
"$TARGET" init >/dev/null
diff_cmd hash-object "$SMALLFILE"
cleanup

setup "git hash-object -w <file>"
SMALLFILE="$ROOT/Cargo.toml"
"$TARGET" init >/dev/null
BLOB=$("$TARGET" hash-object -w "$SMALLFILE")
diff "$SMALLFILE" <(git cat-file -p "$BLOB")
cleanup

setup "git hash-object -w <file> (empty)"
"$TARGET" init >/dev/null
touch foo
diff_cmd hash-object -w foo
cleanup

setup "git ls-tree [--name-only] <tree>"
"$TARGET" init >/dev/null
populate_tree
git add . >/dev/null
TREE=$(git write-tree)
diff_cmd ls-tree --name-only "$TREE"
diff_cmd ls-tree "$TREE"
cleanup

setup "git cat-file -p <tree>"
"$TARGET" init >/dev/null
populate_tree
git add . >/dev/null
TREE=$(git write-tree)
diff_cmd cat-file -p "$TREE"
cleanup

setup "git cat-file -p <commit>"
"$TARGET" init >/dev/null
git commit --allow-empty -mtest-commit >/dev/null
COMMIT=$(git rev-parse HEAD)
diff_cmd cat-file -p "$COMMIT"
cleanup

setup "git cat-file -p <tag>"
"$TARGET" init >/dev/null
git commit --allow-empty -mtest-commit >/dev/null
git tag -a -mtest-msg test-tag
TAG=$(git rev-parse test-tag)
diff_cmd cat-file -p "$TAG"
cleanup

setup "git write-tree"
"$TARGET" init >/dev/null
populate_tree
git add .
diff_cmd write-tree
# check (a few) blobs were created
diff afile <(git cat-file -p $(git hash-object afile))
diff dir/f <(git cat-file -p $(git hash-object dir/f))
# check the (sub)trees were created
git ls-tree -r --name-only $("$TARGET" write-tree) >/dev/null
cleanup

setup "git write-tree (from subdirectory)"
"$TARGET" init >/dev/null
populate_tree
git add .
(cd dir && diff_cmd write-tree)
cleanup
