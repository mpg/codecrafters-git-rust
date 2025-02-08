use anyhow::{bail, Context, Result};
use std::io;
use std::io::prelude::*;

use crate::obj_read::ObjReader;
use crate::obj_type::ObjType;

pub struct TreeReader {
    object: ObjReader,
}

impl TreeReader {
    pub fn from_hash(hash: &str) -> Result<Self> {
        let object = ObjReader::from_hash(hash)?;
        Self::from_object(object)
    }

    pub fn from_object(object: ObjReader) -> Result<Self> {
        if object.obj_type != ObjType::Tree {
            bail!("not a tree");
        }
        Ok(TreeReader { object })
    }

    pub fn print_entries(&mut self, name_only: bool) -> Result<()> {
        while !self.object.eof()? {
            // <mode> <name>\0<20_byte_sha>
            let mode = self
                .object
                .read_up_to(b' ')
                .context("reading tree entry's mode")?;
            let name = self
                .object
                .read_up_to(b'\0')
                .context("reading tree entry's name")?;
            let mut hash = [0u8; 20];
            self.object
                .read_exact(&mut hash)
                .context("reading tree entry's hash")?;

            let mut stdout = io::stdout().lock();
            if !name_only {
                let mode_type = match &mode[..] {
                    b"40000" => "040000 tree",
                    b"100644" => "100644 blob",
                    b"100755" => "100755 blob",
                    b"120000" => "120000 blob",
                    b"160000" => "160000 commit",
                    m => bail!("unknown mode {:?}", m),
                };
                let hash_hex = hex::encode(hash);
                write!(stdout, "{mode_type} {hash_hex}\t")?;
            }
            stdout.write_all(&name)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }

        Ok(())
    }
}
