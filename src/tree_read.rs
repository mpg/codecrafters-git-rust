use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::obj_read::ObjReader;
use crate::obj_type::ObjType;
use crate::tree_entry::Entry;

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

    pub fn print_entries(mut self, name_only: bool) -> Result<()> {
        while !self.object.eof()? {
            let entry = Entry::parse(&mut self.object).context("parsing tree entry")?;
            if name_only {
                entry.print_name()?;
            } else {
                entry.print()?;
            }
        }
        Ok(())
    }

    pub fn actualise_entries(mut self, base_path: &Path) -> Result<()> {
        while !self.object.eof()? {
            let entry = Entry::parse(&mut self.object).context("parsing tree entry")?;
            entry.actualise(base_path)?;
        }
        Ok(())
    }
}
