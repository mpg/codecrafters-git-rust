use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::Path;

use crate::obj_read::ObjReader;
use crate::obj_type::ObjType;
use crate::obj_write::write_object;
use crate::tree_read::TreeReader;
use crate::tree_write::tree_from_workdir;

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

pub fn hash_object(file: &Path, write: bool) -> Result<()> {
    let mut source = fs::File::open(file)
        .with_context(|| format!("could not open {} for reading", file.display()))?;
    let hash_hex = write_object(ObjType::Blob, &mut source, write)?;
    println!("{}", hash_hex);
    Ok(())
}

pub fn ls_tree(tree_hash: &str, name_only: bool) -> Result<()> {
    let mut tree = TreeReader::from_hash(tree_hash)?;
    tree.print_entries(name_only)
}

pub fn write_tree() -> Result<()> {
    let hash = tree_from_workdir()?;
    println!("{hash}");
    Ok(())
}

fn get_env_or(var_name: &str, default: &str) -> String {
    env::var(var_name).unwrap_or(default.into())
}

pub fn commit_tree(tree: &str, parents: &[String], messages: &[String]) -> Result<()> {
    let auth_name = get_env_or("GIT_AUTHOR_NAME", "Author Name");
    let auth_mail = get_env_or("GIT_AUTHOR_EMAIL", "author@example.org");
    let comm_name = get_env_or("GIT_COMMITTER_NAME", "Committer Name");
    let comm_mail = get_env_or("GIT_COMMITTER_EMAIL", "committer@example.org");

    let mut content = Vec::new();

    writeln!(content, "tree {tree}")?;
    for p in parents {
        writeln!(content, "parent {p}")?;
    }
    writeln!(content, "author {auth_name} <{auth_mail}> 0 +0000")?;
    writeln!(content, "committer {comm_name} <{comm_mail}> 0 +0000")?;
    for m in messages {
        writeln!(content, "\n{m}")?;
    }

    let hash = write_object(ObjType::Commit, &mut io::Cursor::new(content), true)?;
    println!("{hash}");
    Ok(())
}
