//! Reader for tree objects

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::obj_read::ObjReader;
use crate::obj_type::ObjType;
use crate::tree_entry::Entry;

/// As simple wrapper for an object reader, with tree-specific methods.
pub struct TreeReader {
    object: ObjReader,
}

impl TreeReader {
    /// Create a tree reader from a tree hash.
    pub fn from_hash(hash: &str) -> Result<Self> {
        let object = ObjReader::from_hash(hash)?;
        Self::from_object(object)
    }

    /// Create a tree reader from an object reader.
    pub fn from_object(object: ObjReader) -> Result<Self> {
        if object.obj_type != ObjType::Tree {
            bail!("not a tree");
        }
        Ok(TreeReader { object })
    }

    /// Print this tree's entries to stdout.
    pub fn print_entries(mut self, name_only: bool) -> Result<()> {
        while !self.object.eof().context("reading tree object")? {
            let entry = Entry::parse(&mut self.object).context("parsing tree entry")?;
            if name_only {
                entry.print_name()?;
            } else {
                entry.print()?;
            }
        }
        Ok(())
    }

    /// Turn this tree object into an actual tree in the filesytem.
    pub fn actualise_entries(mut self, base_path: &Path) -> Result<()> {
        while !self.object.eof().context("reading tree object")? {
            let entry = Entry::parse(&mut self.object).context("parsing tree entry")?;
            entry
                .actualise(base_path)
                .context("creating entry on the filesystem")?;
        }
        Ok(())
    }
}
