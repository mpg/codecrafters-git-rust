use anyhow::{Context, Result};
use std::env;
use std::fs;

fn mkdir(path: &str) -> Result<()> {
    fs::create_dir(path).with_context(|| format!("Could not create directory `{}`", path))
}

fn mkdir_p(path: &str) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("Could not create directory `{}`", path))
}

fn write_file(path: &str, content: &[u8]) -> Result<()> {
    fs::write(path, content).with_context(|| format!("Could not write to file `{}`", path))
}

fn git_init(path: &str) -> Result<()> {
    mkdir_p(path)?;
    let cwd = env::current_dir()?;
    env::set_current_dir(path)?;
    mkdir(".git")?;
    mkdir(".git/objects")?;
    mkdir(".git/refs")?;
    write_file(".git/HEAD", b"ref: refs/heads/main\n")?;
    env::set_current_dir(cwd)?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args[1] == "init" {
        let path = if args.len() == 3 {
            args[2].to_owned()
        } else {
            ".".to_string()
        };
        git_init(&path)?;
        let canon = fs::canonicalize(path)?;
        println!(
            "Initialized empty Git repository in {}/.git/",
            canon.display()
        );
    } else {
        println!("Unknown command: {}", args[1]);
    }

    Ok(())
}
