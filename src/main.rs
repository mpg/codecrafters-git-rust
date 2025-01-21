use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

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
        #[arg(default_value = ".")]
        directory: PathBuf,
    },
    /// Provide contents of repository objects
    CatFile {
        /// Pretty-print the contents of OBJECT based on its type
        #[arg(short = 'p')]
        object: String,
    },
}
use Commands::*;

fn mkdir(path: &Path) -> Result<()> {
    fs::create_dir(path).with_context(|| format!("Could not create directory `{}`", path.display()))
}

fn mkdir_p(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("Could not create directory `{}`", path.display()))
}

fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    fs::write(path, content)
        .with_context(|| format!("Could not write to file `{}`", path.display()))
}

fn git_init(path: &Path) -> Result<()> {
    mkdir_p(path)?;
    mkdir(&path.join(".git"))?;
    mkdir(&path.join(".git/objects"))?;
    mkdir(&path.join(".git/refs"))?;
    write_file(&path.join(".git/HEAD"), b"ref: refs/heads/main\n")?;
    Ok(())
}

fn cat_file_p(object: &str) -> Result<()> {
    let path = &Path::new(".git/objects")
        .join(&object[0..2])
        .join(&object[2..]);
    let compressed = fs::read(path)?;
    let mut z = ZlibDecoder::new(&compressed[..]);
    let mut raw = Vec::<u8>::new();
    z.read_to_end(&mut raw)?;
    let i = raw.iter().position(|&b| b == 0).unwrap(); // TODO
    let s = std::str::from_utf8(&raw[i + 1..])?;
    print!("{}", s);
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
        CatFile { object } => {
            cat_file_p(&object)?;
        }
    }

    Ok(())
}
