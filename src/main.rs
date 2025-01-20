use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::env;
use std::fs;

#[derive(Parser)]
/// A toy implementation of a small subset of git
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create an empty Git repository
    Init {
        /// Directory where the repository should be created
        #[arg(default_value_t = String::from("."))]
        directory: String,
    },
}
use Commands::*;

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
    let args = Cli::parse();
    match args.command {
        Init { directory } => {
            git_init(&directory)?;
            println!(
                "Initialized empty Git repository in {}/.git/",
                fs::canonicalize(directory)?.display()
            );
        }
    }

    Ok(())
}
