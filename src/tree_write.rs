use anyhow::{bail, Context, Result};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::common::git_dir;
use crate::obj_type::ObjType;
use crate::obj_write::write_object;
use crate::tree_entry::{push_tree_entry, Mode};

fn hash_from_dir_entry(entry: &fs::DirEntry) -> Result<String> {
    let path = entry.path();
    let file_type = entry.file_type().context("checking entry type")?;

    if file_type.is_dir() {
        tree_from_dir(&path).context("hashing subtree")
    } else if file_type.is_file() {
        let mut file =
            fs::File::open(&path).with_context(|| format!("could not read {}", path.display()))?;

        write_object(ObjType::Blob, &mut file, true).context("hashing file")
    } else if file_type.is_symlink() {
        let dest = fs::read_link(path).context("readlink")?;
        let mut content = io::Cursor::new(dest.as_os_str().as_bytes());

        write_object(ObjType::Blob, &mut content, true).context("hashing symlink")
    } else {
        bail!("neither a regular file, nor a directory, nor a symlink");
    }
}

const EMPTY_TREE_HASH: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

fn tree_from_dir(dir: &Path) -> Result<String> {
    // We'll need everything in memory so we know the size before writing the object.
    let mut out = Vec::new();

    // We also need the complete list of entries so we can sort it by name.
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name().into_encoded_bytes();
        if name == b".git" {
            continue;
        }

        let hash = hash_from_dir_entry(&entry)?;
        if hash == EMPTY_TREE_HASH {
            continue;
        }

        let mode = Mode::from_dir_entry(&entry)?;

        push_tree_entry(&mut out, mode, &name, &hash);
    }

    write_object(ObjType::Tree, &mut io::Cursor::new(out), true)
}

pub fn tree_from_workdir() -> Result<String> {
    let root = git_dir()?.parent().expect(".git has a parent");
    tree_from_dir(root)
}
