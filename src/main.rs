use anyhow::{anyhow, ensure, Context, Result};
use clap::{Parser, Subcommand};
use flate2::bufread::ZlibDecoder;
use std::fs;
use std::io::{stdout, BufReader, Read, Write};
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

#[derive(Debug, PartialEq, Eq)]
enum ObjType {
    Commit,
    Tree,
    Blob,
    Tag,
}

impl ObjType {
    // Loose objects, once uncompressed, start with either
    // "commit ", "tree ", "blob " or "tag ".
    fn from_stream(s: &mut impl Read) -> Result<ObjType> {
        let mut label = vec![];
        while label.last() != Some(&b' ') {
            let mut buf = [0];
            s.read_exact(&mut buf)?;
            label.push(buf[0]);
        }

        match &label[..label.len() - 1] {
            b"commit" => Ok(ObjType::Commit),
            b"tree" => Ok(ObjType::Tree),
            b"blob" => Ok(ObjType::Blob),
            b"tag" => Ok(ObjType::Tag),
            l => Err(anyhow!("unknown object type {:?}", l)),
        }
    }
}

fn cat_file_p(object: &str) -> Result<()> {
    ensure!(object.len() >= 4, "not a valid object name {}", object);
    let path = &Path::new(".git/objects")
        .join(&object[0..2])
        .join(&object[2..]);
    let file =
        fs::File::open(path).with_context(|| format!("not a valid object name {}", object))?;
    let bufreader = BufReader::new(file);
    let mut zdec = ZlibDecoder::new(bufreader);
    let obj_type = ObjType::from_stream(&mut zdec)
        .with_context(|| format!("could not get type of object {}", object))?;
    ensure!(
        obj_type == ObjType::Blob,
        "cat-file -p only implemented for blobs"
    );
    let mut raw = Vec::<u8>::new();
    zdec.read_to_end(&mut raw)
        .with_context(|| format!("could not decompress {}", object))?;
    let i = raw.iter().position(|&b| b == 0).ok_or(anyhow!(
        "invalid object format: missing nul byte: {}",
        object
    ))?;
    let content = &raw[i + 1..];
    stdout().write_all(content)?;
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
