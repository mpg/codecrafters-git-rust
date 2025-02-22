//! Functions implementing each subcommand from the CLI.

use anyhow::{bail, ensure, Context, Result};
use std::env;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::Path;
use std::time;

use crate::common::git_dir;
use crate::network::{get_pack, ls_remote_head};
use crate::obj_read::ObjReader;
use crate::obj_type::ObjType;
use crate::obj_write::write_object;
use crate::tree_read::TreeReader;
use crate::tree_write::tree_from_workdir;
use crate::unpack::unpack_from;

/// The "git init" command - partial implementation: git populates .git more fully.
pub fn git_init(path: &Path) -> Result<()> {
    let obj_dir = path.join(".git/objects");
    fs::create_dir_all(&obj_dir).with_context(|| format!("creating {}", obj_dir.display()))?;
    fs::create_dir_all(path.join(".git/refs/heads")).context("creating .git/refs/heads")?;
    fs::write(path.join(".git/HEAD"), b"ref: refs/heads/main\n").context("creating .git/HEAD")?;

    println!(
        "Initialized empty Git repository in {}/.git/",
        fs::canonicalize(path)?.display()
    );
    Ok(())
}

/// The "cat-file -p" command.
pub fn cat_file_p(hash: &str) -> Result<()> {
    let mut object =
        ObjReader::from_hash(hash).with_context(|| format!("opening object {hash}"))?;
    match object.obj_type {
        ObjType::Tree => {
            let tree = TreeReader::from_object(object)?;
            tree.print_entries(false)
                .with_context(|| format!("reading & printing tree object {hash}"))?;
        }
        _ => {
            io::copy(&mut object, &mut io::stdout())
                .with_context(|| format!("reading object {hash} to stdout"))?;
        }
    };
    Ok(())
}

/// The "hash-object [-w]" command.
pub fn hash_object(file: &Path, write: bool) -> Result<()> {
    let mut source = fs::File::open(file)
        .with_context(|| format!("could not open {} for reading", file.display()))?;
    let hash_hex = write_object(ObjType::Blob, &mut source, write).context("hashing object")?;
    println!("{}", hash_hex);
    Ok(())
}

/// The "ls-tree [--name-only]" command.
pub fn ls_tree(tree_hash: &str, name_only: bool) -> Result<()> {
    let tree = TreeReader::from_hash(tree_hash)
        .with_context(|| format!("opening tree object {tree_hash}"))?;
    tree.print_entries(name_only)
        .with_context(|| format!("reading & printing tree object {tree_hash}"))?;
    Ok(())
}

/// The "write-tree" command, except it takes the tree directly from the filesystem,
/// bypassing the index. Also, no support for .gitignore either.
pub fn write_tree() -> Result<()> {
    let hash = tree_from_workdir()?;
    println!("{hash}");
    Ok(())
}

fn get_env_or(var_name: &str, default: &str) -> String {
    env::var(var_name).unwrap_or(default.into())
}

// Only support the '@<timestamp> <offset>' format, eg epoch is @0 +0000
fn get_env_date(var_name: &str) -> Option<String> {
    let value = env::var(var_name).ok()?;
    // Sanity-check format: value should start with @
    match value.chars().next() {
        Some('@') => Some(value[1..].into()),
        _ => None,
    }
}

fn get_env_date_or_current(var_name: &str) -> String {
    if let Some(date) = get_env_date(var_name) {
        return date;
    }

    let timestamp = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .expect("live in the present")
        .as_secs();
    format!("{timestamp} +0000")
}

/// The "commit-tree" command, except no support for config: author and commiter details
/// taken either from enviornment variables, or hardcoded defaults.
/// Also, no support for time zones.
pub fn commit_tree(tree: &str, parents: &[String], messages: &[String]) -> Result<()> {
    let auth_name = get_env_or("GIT_AUTHOR_NAME", "Author Name");
    let auth_mail = get_env_or("GIT_AUTHOR_EMAIL", "author@example.org");
    let comm_name = get_env_or("GIT_COMMITTER_NAME", "Committer Name");
    let comm_mail = get_env_or("GIT_COMMITTER_EMAIL", "committer@example.org");

    let auth_date = get_env_date_or_current("GIT_AUTHOR_DATE");
    let comm_date = get_env_date_or_current("GIT_COMMITTER_DATE");

    let mut content = Vec::new();
    writeln!(content, "tree {tree}").context("writing commit contents (tree)")?;
    for p in parents {
        writeln!(content, "parent {p}").context("writing commit contents (parent)")?;
    }
    writeln!(content, "author {auth_name} <{auth_mail}> {auth_date}")
        .context("writing commit contents (author)")?;
    writeln!(content, "committer {comm_name} <{comm_mail}> {comm_date}")
        .context("writing commit contents (committer)")?;
    for m in messages {
        writeln!(content, "\n{m}").context("writing commit contents (message)")?;
    }

    let hash = write_object(ObjType::Commit, &mut io::Cursor::new(content), true)
        .context("writing out commit object")?;
    println!("{hash}");
    Ok(())
}

fn tree_from_commit(commit_hash: &str) -> Result<String> {
    let mut commit = ObjReader::from_hash(commit_hash)
        .with_context(|| format!("opening object {commit_hash}"))?;
    let line = commit
        .read_up_to(b'\n')
        .with_context(|| format!("reading from object {commit_hash}"))?;
    let line =
        String::from_utf8(line).with_context(|| format!("malformed commit {commit_hash}"))?;
    let Some(("tree", tree_hash)) = line.split_once(' ') else {
        bail!("malformed commit {commit_hash}: no tree in first line");
    };
    Ok(tree_hash.into())
}

/// The "checkout-empty" (made up) command - a bit like "checkout" except:
/// - it assumes the working directory is empty, and will overwrite files otherwise;
/// - TODO: it does not update HEAD;
/// - in only accepts an unabbreviate commit hash (no branch name etc.).
pub fn checkout_empty(commit_hash: &str) -> Result<()> {
    let tree_hash = tree_from_commit(commit_hash)
        .with_context(|| format!("getting tree hash from commit {commit_hash}"))?;
    let tree = TreeReader::from_hash(&tree_hash)
        .with_context(|| format!("opening tree object {tree_hash}"))?;
    let root = git_dir()?.parent().expect(".git has a parent");
    tree.actualise_entries(root)
        .with_context(|| format!("checking out to {}", root.display()))?;
    Ok(())
}

/// The "unpack-objects" command - does not support ofs-delta deltified objects.
pub fn unpack_objects() -> Result<()> {
    let nb_obj = unpack_from(io::stdin().lock()).context("unpacking from stdin")?;
    println!("Unpacked {nb_obj} objects");
    Ok(())
}

/// The "ls-remote" command - can only list HEAD.
pub fn ls_remote(repo_url: &str, pattern: &str) -> Result<()> {
    ensure!(pattern == "HEAD", "ls-remote only implemented for HEAD");
    let (hash, _) = ls_remote_head(repo_url).context("listing remote head")?;
    println!("{hash}\tHEAD");
    Ok(())
}

// This seems to be roughly what git is doing based on experiments.
fn dir_from_repo_url(url: &str) -> &Path {
    let url = url.trim_end_matches("/");
    let url = url.trim_end_matches(".git");
    let url = url.trim_end_matches("/");
    let last = url
        .rsplit("/")
        .next()
        .expect("always at least one component");
    Path::new(last)
}

/// The "clone" command. Unlike the real one, it unpacks all object to loose storage.
/// Also, only gets the default branch, not other refs.
/// TODO: does not check if the destination directory is empty.
pub fn clone(repo_url: &str, directory: Option<impl AsRef<Path>>) -> Result<()> {
    let directory = match &directory {
        Some(d) => d.as_ref(),
        None => dir_from_repo_url(repo_url),
    };

    git_init(directory).context("initializing git directory")?;
    env::set_current_dir(directory)
        .with_context(|| format!("changing working directory to {}", directory.display()))?;

    let (head, branch) = ls_remote_head(repo_url).context("listing remote head")?;
    let pack = get_pack(repo_url, &head).context("fetching objects")?;
    let nb_obj = unpack_from(pack).context("unpacking objects")?;
    println!("Unpacked {nb_obj} objects");

    fs::write(".git/HEAD", format!("ref: refs/heads/{branch}\n")).context("updating HEAD")?;
    fs::write(format!(".git/refs/heads/{branch}"), &head)
        .with_context(|| format!("updating branch {branch}"))?;

    checkout_empty(&head).context("checking out HEAD")
}
