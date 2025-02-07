use anyhow::{anyhow, bail, Context, Result};
use std::io::prelude::*;
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

// Read from stream until the given delimiter is found.
// Return content excluding the delimiter.
//
// Somewhat similar to BufRead::read_until(), but we want to use it with
// ZlibDecoder, which does not implement BufRead (though internally buffered).
pub fn read_up_to(s: &mut impl Read, delim: u8) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let mut buf = [0];
        s.read_exact(&mut buf)
            .with_context(|| format!("looking for {:?}", delim))?;
        if buf[0] == delim {
            return Ok(out);
        }
        out.push(buf[0]);
    }
}
