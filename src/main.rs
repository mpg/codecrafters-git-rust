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

fn path_from_hash(hash: &str) -> Result<PathBuf> {
    Ok(git_dir()?
        .join("objects")
        .join(&hash[0..2])
        .join(&hash[2..]))
}

struct ObjReader {
    obj_type: ObjType,
    size: usize,
    used: usize,
    zdec: ZlibDecoder<io::BufReader<fs::File>>,
}

impl ObjReader {
    fn from_hash(hash: &str) -> Result<ObjReader> {
        ensure!(hash.len() >= 4, "not a valid object name {}", hash);
        let obj_path = path_from_hash(hash)?;

        let file = fs::File::open(obj_path)
            .with_context(|| format!("not a valid object name {}", hash))?;
        let bufreader = io::BufReader::new(file);
        let mut zdec = ZlibDecoder::new(bufreader);

        let obj_type = ObjType::from_stream(&mut zdec)
            .with_context(|| format!("could not read type of object {}", hash))?;
        let size = read_obj_size(&mut zdec)
            .with_context(|| format!("could not read size of object {}", hash))?;

        Ok(ObjReader {
            obj_type,
            size,
            used: 0,
            zdec,
        })
    }
}

impl Read for ObjReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.zdec.read(buf) {
            Ok(0) => {
                if self.used == self.size {
                    Ok(0)
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        format!("expected {} bytes, got only {}", self.size, self.used),
                    ))
                }
            }
            Ok(len) => {
                self.used += len;
                if self.used <= self.size {
                    Ok(len)
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("size mismatch: expected {}, got {}+", self.size, self.used),
                    ))
                }
            }
            err => err,
        }
    }
}

fn cat_file_p(hash: &str) -> Result<()> {
    let mut object = ObjReader::from_hash(hash)?;
    match object.obj_type {
        ObjType::Blob => io::copy(&mut object, &mut io::stdout())?,
        _ => bail!("cat-file -p only implemented for blobs"),
    };
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
        let obj_path = path_from_hash(&hash_hex)?;
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
