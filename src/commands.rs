use anyhow::{bail, ensure, Context, Result};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::Path;

use crate::obj_read::ObjReader;
use crate::obj_type::ObjType;
use crate::obj_write::ObjWriter;

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

pub fn cat_file_p(hash: &str) -> Result<()> {
    let mut object = ObjReader::from_hash(hash)?;
    match object.obj_type {
        ObjType::Blob => io::copy(&mut object, &mut io::stdout())
            .with_context(|| format!("reading object {hash} to stdout"))?,
        _ => bail!("cat-file -p only implemented for blobs"),
    };
    Ok(())
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

pub fn ls_tree(tree: &str, name_only: bool) -> Result<()> {
    ensure!(name_only, "only name_only is supported for now");
    let mut object = ObjReader::from_hash(tree)?;
    ensure!(
        object.obj_type == ObjType::Tree,
        format!("not a tree: {}", tree)
    );
    while !object.eof()? {
        // <mode> <name>\0<20_byte_sha>
        let _mode = object.read_up_to(b' ').context("mode")?;
        let name = object.read_up_to(b'\0').context("name")?;
        println!("{}", std::str::from_utf8(&name)?);
        let mut hash = [0u8; 20];
        object.read_exact(&mut hash).context("hash")?;
    }
    Ok(())
}
