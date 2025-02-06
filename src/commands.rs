use anyhow::{anyhow, bail, ensure, Context, Result};
use flate2::{bufread::ZlibDecoder, write::ZlibEncoder, Compression};
use rand::Rng;
use sha1::{Digest, Sha1};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub fn git_init(path: &Path) -> Result<()> {
    let obj_dir = path.join(".git/objects");
    fs::create_dir_all(&obj_dir).with_context(|| format!("creating {}", obj_dir.display()))?;
    fs::create_dir(path.join(".git/refs")).context("creating .git/refs")?;
    fs::write(path.join(".git/HEAD"), b"ref: refs/heads/main\n").context("creating .git/HEAD")?;

    println!(
        "Initialized empty Git repository in {}/.git/",
        fs::canonicalize(path)?.display()
    );
    Ok(())
}

// Read from stream until the given delimiter is found.
// Return content excluding the delimiter.
//
// Somewhat similar to BufRead::read_until(), but we want to use it with
// ZlibDecoder, which does not implement BufRead (though internally buffered).
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

    fn to_str(&self) -> &'static str {
        match self {
            ObjType::Commit => "commit",
            ObjType::Tree => "tree",
            ObjType::Blob => "blob",
            ObjType::Tag => "tag",
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

pub fn cat_file_p(hash: &str) -> Result<()> {
    let mut object = ObjReader::from_hash(hash)?;
    match object.obj_type {
        ObjType::Blob => io::copy(&mut object, &mut io::stdout())
            .with_context(|| format!("reading object {hash} to stdout"))?,
        _ => bail!("cat-file -p only implemented for blobs"),
    };
    Ok(())
}

struct ObjWriter {
    hasher: Sha1,
    zenc: Option<ZlibEncoder<fs::File>>,
    size: usize,
    seen: usize,
    tmp_rand: [u8; 20],
}

impl ObjWriter {
    fn tmp_path(tmp_rand: &[u8]) -> Result<PathBuf> {
        let tmp_name = format!("tmpobj{}", hex::encode(tmp_rand));
        Ok(git_dir()?.join(tmp_name))
    }

    fn new(obj_type: ObjType, size: usize, write: bool) -> Result<ObjWriter> {
        let hasher = Sha1::new();

        let mut tmp_rand = [0u8; 20];
        if write {
            rand::rng().fill(&mut tmp_rand);
        }
        let zenc = if write {
            // We don't know the name (hash) yet, so use a temporary file
            let tmp_path = Self::tmp_path(&tmp_rand)?;
            let file = fs::File::create(&tmp_path)
                .with_context(|| format!("could not create {}", tmp_path.display()))?;

            let mut perms = file.metadata()?.permissions();
            perms.set_readonly(true);
            fs::set_permissions(tmp_path, perms)?;

            Some(ZlibEncoder::new(file, Compression::default()))
        } else {
            None
        };

        let mut writer = ObjWriter {
            hasher,
            zenc,
            size,
            seen: 0,
            tmp_rand,
        };
        write!(writer, "{} {}\0", obj_type.to_str(), size).context("writing object header")?;
        writer.seen = 0;
        Ok(writer)
    }

    fn finish(self) -> Result<String> {
        if self.seen != self.size {
            bail!("size mismatch: expected {}, got {}", self.size, self.seen);
        }

        let hash_bin = self.hasher.finalize();
        let hash_hex = format!("{:x}", hash_bin);

        if let Some(zenc) = self.zenc {
            zenc.finish().context("closing zlib stream")?;
            let from = Self::tmp_path(&self.tmp_rand)?;
            let to = path_from_hash(&hash_hex)?;
            fs::create_dir_all(to.parent().unwrap())
                .with_context(|| format!("creating {}", to.parent().unwrap().display()))?;
            fs::rename(from, &to)
                .with_context(|| format!("renaming temporary file to {}", to.display()))?;
        }

        Ok(hash_hex)
    }
}

impl Write for ObjWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = if let Some(zenc) = &mut self.zenc {
            zenc.write(buf)?
        } else {
            buf.len()
        };

        self.hasher.update(&buf[..n]);
        self.seen += n;

        if self.seen > self.size {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("size mismatch: expected {}, got {}+", self.size, self.seen),
            ))
        } else {
            Ok(n)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.zenc {
            Some(z) => z.flush(),
            None => Ok(()),
        }
    }
}

pub fn hash_object(file: &Path, write: bool) -> Result<()> {
    let mut source =
        fs::File::open(file).with_context(|| format!("could not read {}", file.display()))?;
    let size = source.metadata()?.len() as usize;

    let mut object = ObjWriter::new(ObjType::Blob, size, write)?;
    io::copy(&mut source, &mut object)
        .with_context(|| format!("copying from {} to object", file.display()))?;
    let hash_hex = object
        .finish()
        .context("writing out object to final location")?;

    println!("{}", hash_hex);
    Ok(())
}
