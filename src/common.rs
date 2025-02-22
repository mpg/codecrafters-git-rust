//! Basic functions used by several other modules.

use anyhow::{anyhow, bail, Context, Result};
use std::path::PathBuf;
use std::sync::LazyLock;

static GIT_DIR: LazyLock<Result<PathBuf>> = LazyLock::new(|| {
    let cwd = std::env::current_dir().context("getting current directory (looking for .git)")?;
    for dir in cwd.ancestors() {
        if dir.join(".git").is_dir() {
            return Ok(dir.join(".git"));
        }
    }
    bail!("not a git repository (or any of the parent directories): .git");
});

/// Return the path to the .git directory, for example "/path/to/repo/.git".
pub fn git_dir() -> Result<&'static PathBuf> {
    GIT_DIR.as_ref().map_err(|e| anyhow!(e.to_string()))
}

/// Return the path for an object identified by its hash.
/// For example, "/path/to/repo/.git/objects/01/2345...40".
pub fn path_from_hash(hash: &str) -> Result<PathBuf> {
    Ok(git_dir()?
        .join("objects")
        .join(&hash[0..2])
        .join(&hash[2..]))
}
