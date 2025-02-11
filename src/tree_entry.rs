use anyhow::{bail, Context, Result};

use std::fs;
use std::io;
use std::io::prelude::*;
use std::os::unix::fs::PermissionsExt;

use crate::obj_read::ObjReader;
use crate::obj_type::ObjType;

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
