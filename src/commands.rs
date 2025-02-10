use anyhow::{bail, Context, Result};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::common::git_dir;
use crate::obj_read::ObjReader;
use crate::obj_type::ObjType;
use crate::obj_write::ObjWriter;
use crate::tree_entry::Mode;
use crate::tree_read::TreeReader;

pub fn git_init(path: &Path) -> Result<()> {
    let obj_dir = path.join(".git/objects");
    fs::create_dir_all(&obj_dir).with_context(|| format!("creating {}", obj_dir.display()))?;
    fs::create_dir(path.join(".git/refs")).context("creating .git/refs")?;
    fs::write(path.join(".git/HEAD"), b"ref: refs/heads/main\n").context("creating .git/HEAD")?;

    println!(
        "Initialized empty Git repository in {}/.git/",
        fs::canonicalize(path)?.display()
    );
    Ok(())
}

pub fn cat_file_p(hash: &str) -> Result<()> {
    let mut object = ObjReader::from_hash(hash)?;
    match object.obj_type {
        ObjType::Tree => {
            let mut tree = TreeReader::from_object(object)?;
            tree.print_entries(false)?;
        }
        _ => {
            io::copy(&mut object, &mut io::stdout())
                .with_context(|| format!("reading object {hash} to stdout"))?;
        }
    };
    Ok(())
}

fn blob_from_file(file: &Path, write: bool) -> Result<String> {
    let mut source =
        fs::File::open(file).with_context(|| format!("could not read {}", file.display()))?;
    let size = source.metadata()?.len() as usize;

    let mut object = ObjWriter::new(ObjType::Blob, size, write)?;
    io::copy(&mut source, &mut object)
        .with_context(|| format!("copying from {} to object", file.display()))?;
    object
        .finish()
        .context("writing out object to final location")
}

pub fn hash_object(file: &Path, write: bool) -> Result<()> {
    let hash_hex = blob_from_file(file, write)?;
    println!("{}", hash_hex);
    Ok(())
}

pub fn ls_tree(tree_hash: &str, name_only: bool) -> Result<()> {
    let mut tree = TreeReader::from_hash(tree_hash)?;
    tree.print_entries(name_only)
}

fn blob_from_symlink(link: &Path) -> Result<String> {
    let dest = fs::read_link(link).context("readlink")?;
    let content = dest.as_os_str().as_bytes();
    let mut object = ObjWriter::new(ObjType::Blob, content.len(), true)?;
    object.write_all(content)?;
    object.finish()
}

fn hash_from_dir_entry(entry: &fs::DirEntry) -> Result<String> {
    let file_type = entry.file_type().context("checking entry type")?;
    if file_type.is_file() {
        blob_from_file(&entry.path(), true).context("hashing file")
    } else if file_type.is_dir() {
        tree_from_dir(&entry.path()).context("hashing subtree")
    } else if file_type.is_symlink() {
        blob_from_symlink(&entry.path()).context("hashing symlink")
    } else {
        bail!("neither a regular file, nor a directory, nor a symlink");
    }
}

fn write_dir_entry(mode: Mode, name: &[u8], hash: &str, out: &mut Vec<u8>) {
    let hash = hex::decode(hash).expect("hash is valid hex");

    out.extend_from_slice(mode.to_str().as_bytes());
    out.push(b' ');
    out.extend_from_slice(name);
    out.push(b'\0');
    out.extend_from_slice(&hash);
}

const EMPTY_TREE_HASH: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

fn tree_from_dir(dir: &Path) -> Result<String> {
    // We'll need everything in memory so we know the size before writing the object.
    let mut out = Vec::new();

    // We also need the complete list of entries so we can sort it by name.
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, io::Error>>()?;
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

        write_dir_entry(mode, &name, &hash, &mut out);
    }

    let mut writer = ObjWriter::new(ObjType::Tree, out.len(), true).context("new")?;
    writer.write_all(&out).context("write_all")?;
    writer.finish()
}

pub fn write_tree() -> Result<()> {
    let root = git_dir()?.parent().expect(".git has a parent");
    let hash = tree_from_dir(root)?;
    println!("{hash}");
    Ok(())
}
