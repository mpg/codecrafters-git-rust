//! A toy implementation of a small subset of git.
//!
//! See [Commands] for the list of git sub-commands (partially) implemented.
//!
//! Major restrictions (within the subset of commands implemented):
//! - Only works with loose objects (ie will not work after git gc).
//! - No index (stating area), no support for .gitignore.
//! - No support for git config (only environment variables for author etc.).
//! - The checkout-empty command will happily overwrite files if the directory's not empty.
//! - Hashes may not be abbreviated; using references (eg branch names) is not supported.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

// Use a flat structure
mod commands;
mod common;
mod network;
mod obj_read;
mod obj_type;
mod obj_write;
mod tree_entry;
mod tree_read;
mod tree_write;
mod unpack;

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
    /// List the contents of a tree object
    LsTree {
        /// List only filenames, one per line
        #[arg(long)]
        name_only: bool,
        /// The tree object to list
        tree: String,
    },
    /// Create a tree object from the current directory (not index)
    WriteTree,
    /// Create a new commit object
    CommitTree {
        /// Each -p indicates the id of a parent commit object
        #[arg(short)]
        parent: Vec<String>,
        /// A paragraph in the commit log message
        #[arg(short, required = true)]
        message: Vec<String>,
        /// An existing tree object
        tree: String,
    },
    /// Write out working tree files from a commit (assumes an empty workdir)
    CheckoutEmpty {
        /// The commit for check out
        commit: String,
    },
    /// Unpack objects from a packed archive
    UnpackObjects,
    /// List references in a remote repository (only HEAD supported)
    LsRemote {
        /// The remote repository URL (must be HTTP)
        repo: String,
        /// The reference to list (must be HEAD)
        pattern: String,
    },
    /// Clone a repository into a new directory
    Clone {
        /// The remote repository URL (must be HTTP)
        repo: String,
        /// The target directory (will be created if needed)
        directory: Option<PathBuf>,
    },
}
use Commands::*;

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    match args.command {
        Init { directory } => git_init(&directory)?,
        CatFile { object } => cat_file_p(&object)?,
        HashObject { write, file } => hash_object(&file, write)?,
        LsTree { name_only, tree } => ls_tree(&tree, name_only)?,
        WriteTree => write_tree()?,
        CommitTree {
            parent,
            message,
            tree,
        } => commit_tree(&tree, &parent, &message)?,
        CheckoutEmpty { commit } => checkout_empty(&commit)?,
        UnpackObjects => unpack_objects()?,
        LsRemote { repo, pattern } => ls_remote(&repo, &pattern)?,
        Clone { repo, directory } => clone(&repo, directory.as_ref())?,
    }

    Ok(())
}
