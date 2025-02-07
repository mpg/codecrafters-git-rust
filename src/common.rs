use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;
use std::sync::LazyLock;

static GIT_DIR: LazyLock<Result<PathBuf>> = LazyLock::new(|| {
    let cwd = std::env::current_dir()?;
    for dir in cwd.ancestors() {
        if dir.join(".git").is_dir() {
            return Ok(dir.join(".git"));
        }
    }
    bail!("not a git repository (or any of the parent directories): .git");
});

pub fn git_dir() -> Result<&'static PathBuf> {
    GIT_DIR.as_ref().map_err(|e| anyhow!(e.to_string()))
}

pub fn path_from_hash(hash: &str) -> Result<PathBuf> {
    Ok(git_dir()?
        .join("objects")
        .join(&hash[0..2])
        .join(&hash[2..]))
}
