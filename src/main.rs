use anyhow::{anyhow, bail, ensure, Context, Result};
use clap::{Parser, Subcommand};
use flate2::{bufread::ZlibDecoder, write::ZlibEncoder, Compression};
use sha1::{Digest, Sha1};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

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

// Read from stream until the given delimiter is found.
// Return content excluding the delimiter.
fn read_up_to(s: &mut impl Read, delim: u8) -> Result<Vec<u8>> {
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

#[derive(Debug, PartialEq, Eq)]
enum ObjType {
    Commit,
    Tree,
    Blob,
    Tag,
}

impl ObjType {
    // Loose objects, once uncompressed, start with either
    // "commit", "tree", "blob" or "tag", followed by a " ".
    fn from_stream(s: &mut impl Read) -> Result<ObjType> {
        let label = read_up_to(s, b' ')?;
        match label.as_slice() {
            b"commit" => Ok(ObjType::Commit),
            b"tree" => Ok(ObjType::Tree),
            b"blob" => Ok(ObjType::Blob),
            b"tag" => Ok(ObjType::Tag),
            l => Err(anyhow!("unknown object type {:?}", l)),
        }
    }
}

// Read the size field of a loose object: ascii digits terminated by '\0'.
fn read_obj_size(s: &mut impl Read) -> Result<usize> {
    let digits = read_up_to(s, b'\0')?;
    let size: usize = std::str::from_utf8(&digits)?.parse()?;
    Ok(size)
}

static GIT_DIR: LazyLock<Result<PathBuf>> = LazyLock::new(|| {
    let cwd = std::env::current_dir()?;
    for dir in cwd.ancestors() {
        if dir.join(".git").is_dir() {
            return Ok(dir.join(".git"));
        }
    }
    bail!("not a git repository (or any of the parent directories): .git");
});

fn git_dir() -> Result<&'static PathBuf> {
    GIT_DIR.as_ref().map_err(|e| anyhow!(e.to_string()))
}

struct Object {
    obj_type: ObjType,
    content: Vec<u8>,
}

impl Object {
    fn from_hash(hash: &str) -> Result<Object> {
        ensure!(hash.len() >= 4, "not a valid object name {}", hash);
        let obj_path = git_dir()?
            .join("objects")
            .join(&hash[0..2])
            .join(&hash[2..]);

        let file = fs::File::open(obj_path)
            .with_context(|| format!("not a valid object name {}", hash))?;
        let bufreader = io::BufReader::new(file);
        let mut zdec = ZlibDecoder::new(bufreader);

        let obj_type = ObjType::from_stream(&mut zdec)
            .with_context(|| format!("could not read type of object {}", hash))?;
        let size = read_obj_size(&mut zdec)
            .with_context(|| format!("could not read size of object {}", hash))?;

        let mut content = Vec::<u8>::new();
        zdec.read_to_end(&mut content)
            .with_context(|| format!("could not read content of object {}", hash))?;

        ensure!(
            size == content.len(),
            format!("size mismatch for object {}", hash)
        );

        Ok(Object { obj_type, content })
    }
}

fn cat_file_p(hash: &str) -> Result<()> {
    let object = Object::from_hash(hash)?;
    match object.obj_type {
        ObjType::Blob => io::stdout().write_all(&object.content)?,
        _ => bail!("cat-file -p only implemented for blobs"),
    }
    Ok(())
}

fn hash_object(file: &Path, write: bool) -> Result<()> {
    let mut raw = Vec::new();
    raw.extend_from_slice(b"blob ");

    let mut source =
        fs::File::open(file).with_context(|| format!("could not read {}", file.display()))?;
    let len = source.metadata()?.len();
    raw.extend_from_slice(len.to_string().as_bytes());
    raw.push(0);

    source
        .read_to_end(&mut raw)
        .with_context(|| format!("error reading {}", file.display()))?;

    let hash_bin = Sha1::digest(&raw);
    let hash_hex = hex::encode(hash_bin);

    if write {
        let obj_path = git_dir()?
            .join("objects")
            .join(&hash_hex[0..2])
            .join(&hash_hex[2..]);
        mkdir_p(obj_path.parent().unwrap())?;

        let output = fs::File::create(&obj_path)
            .with_context(|| format!("could not create {}", obj_path.display()))?;
        let mut perms = output.metadata()?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(obj_path, perms)?;

        let mut zenc = ZlibEncoder::new(output, Compression::default());
        zenc.write_all(&raw)?;
    }

    println!("{}", hash_hex);
    Ok(())
}

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
        HashObject { write, file } => {
            hash_object(&file, write)?;
        }
    }

    Ok(())
}
