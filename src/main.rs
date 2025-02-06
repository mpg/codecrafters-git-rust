use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod commands;
use commands::*;

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
    /// Compute object hash and optionally create an object from a file
    HashObject {
        /// Actually write the object into the object database
        #[arg(short)]
        write: bool,
        /// File to read
        file: PathBuf,
    },
}
use Commands::*;

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    match args.command {
        Init { directory } => git_init(&directory)?,
        CatFile { object } => cat_file_p(&object)?,
        HashObject { write, file } => hash_object(&file, write)?,
    }

    Ok(())
}
