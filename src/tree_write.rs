//! Writing tree objects

use anyhow::{bail, Context, Result};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::common::git_dir;
use crate::obj_type::ObjType;
use crate::obj_write::write_object;
use crate::tree_entry::{Entry, Mode};

/// Hash and write to object storage the given entry.
fn hash_entry(path: &Path, meta: &fs::Metadata) -> Result<String> {
    if meta.is_dir() {
        tree_from_dir(path).context("hashing subtree")
    } else if meta.is_file() {
        let mut file =
            fs::File::open(path).with_context(|| format!("could not read {}", path.display()))?;

        write_object(ObjType::Blob, &mut file, true).context("hashing file")
    } else if meta.is_symlink() {
        let dest = fs::read_link(path).context("readlink")?;
        let mut content = io::Cursor::new(dest.as_os_str().as_bytes());

        write_object(ObjType::Blob, &mut content, true).context("hashing symlink")
    } else {
        bail!("neither a regular file, nor a directory, nor a symlink");
    }
}

/// The hash of the empty tree, used to detect and skip them.
///
/// This is more convenient than checking using read_dir as we need to
/// ignore .git and recursively ignore "empty" directories.
const EMPTY_TREE_HASH: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Get entries for the given directory, sorted how git wants them.
fn sorted_entries(dir: &Path) -> Result<Vec<(fs::DirEntry, fs::Metadata)>> {
    let mut entries = Vec::new();
    let iter =
        fs::read_dir(dir).with_context(|| format!("read_dir failed for {}", dir.display()))?;
    for entry in iter {
        let entry = entry.with_context(|| format!("bad direntry in {}", dir.display()))?;
        let meta = entry
            .metadata()
            .with_context(|| format!("metadata for {}", entry.path().display()))?;
        entries.push((entry, meta));
    }

    // Sort as if directories had a '/' appended to their name.
    entries.sort_unstable_by(|a, b| {
        let (mut a_name, a_is_dir) = (a.0.file_name().into_encoded_bytes(), a.1.is_dir());
        let (mut b_name, b_is_dir) = (b.0.file_name().into_encoded_bytes(), b.1.is_dir());

        if a_is_dir {
            a_name.push(b'/');
        }
        if b_is_dir {
            b_name.push(b'/');
        }

        a_name.cmp(&b_name)
    });

    Ok(entries)
}

/// Create a tree object for the given directory and return its hash.
fn tree_from_dir(dir: &Path) -> Result<String> {
    // We'll need everything in memory so we know the size before writing the object.
    let mut out = Vec::new();

    let entries = sorted_entries(dir)?;

    for (entry, meta) in entries {
        let name = entry.file_name().into_encoded_bytes();
        if name == b".git" {
            continue;
        }

        let hash = hash_entry(&entry.path(), &meta)?;
        if hash == EMPTY_TREE_HASH {
            continue;
        }
        let hash = hex::decode(hash).expect("hash is valid hex");
        let hash: [u8; 20] = hash.try_into().expect("hash is 20 bytes");

        let mode = Mode::from_metadata(&meta)?;

        Entry { mode, name, hash }.push_to_vec(&mut out);
    }

    write_object(ObjType::Tree, &mut io::Cursor::new(out), true)
}

/// Create a tree object for the git working directory and return its hash.
pub fn tree_from_workdir() -> Result<String> {
    let root = git_dir()?.parent().expect(".git has a parent");
    tree_from_dir(root)
}
