#!/usr/bin/bash

set -eu

cargo build
cargo fmt --check
cargo clippy -- -D warnings

ROOT="$PWD"
TARGET="$ROOT/target/debug/codecrafters-git"
TESTDIR="$ROOT/test-$$"
OTHERDIR="$ROOT/test2-$$"
cleanup() {
    cd "$ROOT"
    rm -rf "$TESTDIR" "$OTHERDIR"
}
setup() {
    echo "$1"
    mkdir "$OTHERDIR"
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

dodir() {
    # create non-empty dir, so that git sees it
    mkdir $1 && touch $1/x
}

populate_tree_tricky_sort() {
    # same length, alternating types
    touch 0a
    dodir 0b
    touch 0c
    dodir 0d
    # same length, non-alternating types
    touch 1a
    touch 1b
    dodir 1c
    dodir 1d
    # prefixes, alternating
    touch 2a
    dodir 2ab
    touch 2abc
    dodir 2abcd
    # prefixes, non-alternating
    touch 3a
    touch 3ab
    dodir 3abc
    dodir 3abcd
    # around '/', dir just shorter
    touch 4.
    dodir 4
    touch 40
    # around '/', all files
    touch 5.
    touch 5
    touch 50
    # around '/', all dirs
    dodir 6.
    dodir 6
    dodir 60
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

setup "git write-tree (empty)"
"$TARGET" init >/dev/null
diff_cmd write-tree
cleanup

setup "git write-tree (tricky sorting rules)"
"$TARGET" init >/dev/null
populate_tree_tricky_sort
git add .
diff_cmd write-tree
cleanup

setup "git commit-tree <tree> -m <message> [-p <parent>]"
"$TARGET" init >/dev/null
TREE=$("$TARGET" write-tree)
COMMIT_1=$("$TARGET" commit-tree "$TREE" -m initial)
COMMIT_2=$("$TARGET" commit-tree "$TREE" -m second -m blah -p "$COMMIT_1")
git show "$COMMIT_1" >/dev/null
git show "$COMMIT_2" >/dev/null
git log "$COMMIT_2" >/dev/null
cleanup

setup "git commit-tree <tree> -m <message>... (environment)"
"$TARGET" init >/dev/null
TREE=$("$TARGET" write-tree)
(
    export GIT_AUTHOR_NAME="A. Hacker"
    export GIT_AUTHOR_EMAIL="hacker@example.org"
    export GIT_AUTHOR_DATE="@0 +0000"
    export GIT_COMMITTER_NAME="A. Maintainer"
    export GIT_COMMITTER_EMAIL="maint@example.org"
    export GIT_COMMITTER_DATE="@86400 +0000"
    diff_cmd commit-tree -m "test commit" -m "second paragraph" -m "third" "$TREE"
)
cleanup

setup "git checkout-empty <commit>"
"$TARGET" init >/dev/null
populate_tree
rm -r ignored-dir
TREE=$("$TARGET" write-tree)
COMMIT=$("$TARGET" commit-tree "$TREE" -m initial)
(
    cd "$OTHERDIR"
    cp -a "$TESTDIR/.git" .
    "$TARGET" checkout-empty "$COMMIT"
)
diff <(ls -lR) <(cd "$OTHERDIR" && ls -lR)
cleanup

setup "git unpack-objects (undeltified, 2 blobs)"
git init >/dev/null
FILE1="$ROOT"/your_program.sh
FILE2="$ROOT"/Cargo.toml
BLOB1=$(git hash-object -w "$FILE1")
BLOB2=$(git hash-object -w "$FILE2")
printf "$BLOB1\n$BLOB2\n" | git pack-objects -q --depth=0 --stdout >mypack
rm -rf .git
"$TARGET" init >/dev/null
"$TARGET" unpack-objects < mypack >/dev/null
diff <(git cat-file -p $BLOB1) "$FILE1"
diff <(git cat-file -p $BLOB2) "$FILE2"
cleanup

setup "git unpack-objects (undeltified, real-world)"
(cd "$ROOT" && git pack-objects --all --depth=0 -q --stdout </dev/null) > mypack
"$TARGET" init >/dev/null
"$TARGET" unpack-objects < mypack >/dev/null
COMMIT=$(cd "$ROOT" && git rev-parse HEAD)
git cat-file commit "$COMMIT" >/dev/null
cleanup

setup "git unpack-objects (deltified: copy only)"
# This will store B (longest) in full and for A use a single copy instruction.
git init >/dev/null
cp "$ROOT/your_program.sh" a
cp "$ROOT/your_program.sh" b
echo "# bla" >> b
A=$(git hash-object -w a)
B=$(git hash-object -w b)
printf "$A\n$B\n" | git pack-objects -q --stdout >mypack
rm -rf .git
"$TARGET" init >/dev/null
"$TARGET" unpack-objects < mypack >/dev/null
diff <(git cat-file -p $A) a
diff <(git cat-file -p $B) b
cleanup

setup "git unpack-objects (deltified: copy-add-copy)"
# This will store one of them and for the other copy-add-copy.
cp "$ROOT/your_program.sh" a
cp "$ROOT/your_program.sh" b
sed -i 's/Copied/COPIED/' b
A=$(git hash-object -w a)
B=$(git hash-object -w b)
printf "$A\n$B\n" | git pack-objects -q --stdout >mypack
rm -rf .git
"$TARGET" init >/dev/null
"$TARGET" unpack-objects < mypack >/dev/null
diff <(git cat-file -p $A) a
diff <(git cat-file -p $B) b
cleanup

setup "git unpack-objects (deltified, real-world)"
(cd "$ROOT" && git pack-objects --all -q --stdout </dev/null) > mypack
"$TARGET" init >/dev/null
"$TARGET" unpack-objects < mypack >/dev/null
COMMIT=$(cd "$ROOT" && git rev-parse HEAD)
git cat-file commit "$COMMIT" >/dev/null
cleanup
