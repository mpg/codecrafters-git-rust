use anyhow::{bail, Context, Result};
use flate2::bufread::ZlibDecoder;
use std::io;
use std::io::prelude::*;

use crate::obj_type::ObjType;
use crate::obj_write::ObjWriter;

// gitformat-pack(5) "object entries, each of which looks like this"
fn unpack_object(reader: &mut impl BufRead) -> Result<()> {
    // n-byte type and length (3-bit type, (n-1)*7+4-bit length)
    let mut buf = [0u8];
    reader
        .read_exact(&mut buf)
        .context("reading object's first byte")?;
    let type_id = (buf[0] >> 4) & 0b111;
    let obj_type = match type_id {
        1 => ObjType::Commit,
        2 => ObjType::Tree,
        3 => ObjType::Blob,
        4 => ObjType::Tag,
        _ => bail!("deltified representation not supported yet"),
    };

    // gitformat-pack(5) "Size encoding"
    let mut size = (buf[0] & 0x0f) as usize;
    let mut bits = 4;
    while buf[0] & 0x80 != 0 {
        reader
            .read_exact(&mut buf)
            .context("reading object's size")?;
        size += ((buf[0] & 0x7f) as usize) << bits;
        bits += 7;
    }

    // compressed data
    let mut zdec = ZlibDecoder::new(reader);
    let mut object = ObjWriter::new(obj_type, size, true).context("creating object")?;
    io::copy(&mut zdec, &mut object).context("copying data to object")?;
    object
        .finish()
        .context("writing object to object database")?;

    Ok(())
}

// gitformat-pack(5) "pack-*.pack files have the following format"
pub fn unpack_from<R: BufRead>(mut reader: R) -> Result<u32> {
    // 4-byte signature "PACK" + 4-byte version number 2
    // 4-byte number of objects
    let mut head = [0u8; 12];
    reader
        .read_exact(&mut head)
        .context("reading packfile header")?;
    if &head[..8] != b"PACK\x00\x00\x00\x02" {
        bail!("invalid packfile header: {:?}", head);
    }
    let last4 = head[8..12].try_into().expect("slice size is 4");
    let nb_obj = u32::from_be_bytes(last4);

    // object entries
    for i in 0..nb_obj {
        unpack_object(&mut reader)
            .with_context(|| format!("unpacking object {}/{}", i + 1, nb_obj))?;
    }

    // pack checksum
    let mut foot = [0u8; 20];
    reader
        .read_exact(&mut foot)
        .context("reading packfile checksum")?;
    // should check the value, but for now just check we've reached EOF
    let Ok(0) = reader.read(&mut [0]) else {
        bail!("trailing data at end of packfile");
    };

    Ok(nb_obj)
}
