use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::os::unix;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::obj_read::ObjReader;
use crate::obj_type::ObjType;
use crate::tree_read::TreeReader;

pub enum Mode {
    Dir,
    File,
    Exe,
    SymLink,
    SubMod,
}

impl Mode {
    fn from_bytes(mode: &[u8]) -> Result<Self> {
        match mode {
            b"40000" => Ok(Mode::Dir),
            b"100644" => Ok(Mode::File),
            b"100755" => Ok(Mode::Exe),
            b"120000" => Ok(Mode::SymLink),
            b"160000" => Ok(Mode::SubMod),
            m => bail!("unknown mode {:?}", m),
        }
    }

    pub fn from_metadata(meta: &fs::Metadata) -> Result<Self> {
        if meta.is_file() {
            if meta.permissions().mode() & 0o111 != 0 {
                Ok(Mode::Exe)
            } else {
                Ok(Mode::File)
            }
        } else if meta.is_dir() {
            Ok(Mode::Dir)
        } else if meta.is_symlink() {
            Ok(Mode::SymLink)
        } else {
            bail!("neither a regular file, nor a directory, nor a symlink");
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Mode::Dir => "40000",
            Mode::File => "100644",
            Mode::Exe => "100755",
            Mode::SymLink => "120000",
            Mode::SubMod => "160000",
        }
    }

    fn obj_type(&self) -> ObjType {
        match self {
            Mode::Dir => ObjType::Tree,
            Mode::File => ObjType::Blob,
            Mode::Exe => ObjType::Blob,
            Mode::SymLink => ObjType::Blob,
            Mode::SubMod => ObjType::Commit,
        }
    }
}

pub struct Entry {
    mode: Mode,
    name: Vec<u8>,
    hash: [u8; 20],
}

impl Entry {
    pub fn parse(object: &mut ObjReader) -> Result<Self> {
        // <mode> <name>\0<20_byte_sha>
        let mode = object.read_up_to(b' ').context("reading mode")?;
        let name = object.read_up_to(b'\0').context("reading name")?;
        let mut hash = [0u8; 20];
        object.read_exact(&mut hash).context("reading hash")?;

        let mode = Mode::from_bytes(&mode)?;
        Ok(Entry { mode, name, hash })
    }

    pub fn print_name(&self) -> Result<()> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(&self.name)?;
        stdout.write_all(b"\n")?;
        Ok(())
    }

    pub fn print(&self) -> Result<()> {
        let mut stdout = io::stdout().lock();
        let mode = self.mode.to_str();
        let otype = self.mode.obj_type().to_str();
        let hash = hex::encode(self.hash);
        write!(stdout, "{mode:0>6} {otype} {hash}\t")?;
        stdout.write_all(&self.name)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(())
    }

    pub fn actualise(&self, base_path: &Path) -> Result<()> {
        let hash = hex::encode(self.hash);
        let mut object =
            ObjReader::from_hash(&hash).with_context(|| format!("opening object {hash}"))?;
        let path = base_path.join(OsStr::from_bytes(&self.name));

        match self.mode {
            Mode::Dir => {
                fs::create_dir(&path)
                    .with_context(|| format!("creating directory {}", path.display()))?;
                let tree = TreeReader::from_object(object)?;
                tree.actualise_entries(&path)
                    .with_context(|| format!("checking out, subdr {}", path.display()))?;
            }
            Mode::File | Mode::Exe => {
                let mut out = fs::File::create(&path)
                    .with_context(|| format!("creating file {}", path.display()))?;
                io::copy(&mut object, &mut out)
                    .with_context(|| format!("copying object {hash} to file {}", path.display()))?;
                if let Mode::Exe = self.mode {
                    let meta = out
                        .metadata()
                        .with_context(|| format!("stat {}", path.display()))?;
                    let mut perms = meta.permissions();
                    perms.set_mode(perms.mode() | 0o111);
                    fs::set_permissions(&path, perms)
                        .with_context(|| format!("making {} executable", path.display()))?;
                }
            }
            Mode::SymLink => {
                let mut target = Vec::new();
                io::copy(&mut object, &mut target)
                    .with_context(|| format!("reading from object {hash}"))?;
                let target = OsStr::from_bytes(&target);
                unix::fs::symlink(target, &path).with_context(|| {
                    format!(
                        "creating symlink {} -> {}",
                        path.display(),
                        Path::new(target).display()
                    )
                })?;
            }
            Mode::SubMod => {
                bail!("support for submodule not implemented");
            }
        }

        Ok(())
    }
}

pub fn push_tree_entry(out: &mut Vec<u8>, mode: Mode, name: &[u8], hash: &str) {
    let hash = hex::decode(hash).expect("hash is valid hex");

    // <mode> <name>\0<20_byte_sha>
    out.extend_from_slice(mode.to_str().as_bytes());
    out.push(b' ');
    out.extend_from_slice(name);
    out.push(b'\0');
    out.extend_from_slice(&hash);
}
