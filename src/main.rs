use anyhow::{Context, Result};
use std::env;
use std::fs;

fn mkdir(path: &str) -> Result<()> {
    fs::create_dir(path).with_context(|| format!("Could not create directory `{}`", path))
}

fn write_file(path: &str, content: &[u8]) -> Result<()> {
    fs::write(path, content).with_context(|| format!("Could not write to file `{}`", path))
}

fn git_init() -> Result<()> {
    mkdir(".git")?;
    mkdir(".git/objects")?;
    mkdir(".git/refs")?;
    write_file(".git/HEAD", b"ref: refs/heads/main\n")?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args[1] == "init" {
        git_init()?;
        println!("Initialized git directory");
    } else {
        println!("Unknown command: {}", args[1]);
    }

    Ok(())
}
