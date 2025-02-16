use anyhow::{bail, Context, Result};
use flate2::bufread::ZlibDecoder;
use sha1::{Digest, Sha1};
use std::io;
use std::io::prelude::*;

use crate::obj_read::ObjReader;
use crate::obj_type::ObjType;
use crate::obj_write::ObjWriter;

struct HashingReader<R> {
    hasher: Sha1,
    reader: R,
}

impl<R: BufRead> HashingReader<R> {
    fn new(reader: R) -> Self {
        let hasher = Sha1::new();
        Self { hasher, reader }
    }

    fn finish(mut self) -> Result<()> {
        let hash = self.hasher.finalize();
        let mut foot = [0u8; 20];
        self.reader
            .read_exact(&mut foot)
            .context("reading final checksum")?;
        if hash != foot.into() {
            bail!("checksum mismatch: exp {hash:?}, got {foot:?}");
        }
        let Ok(0) = self.reader.read(&mut [0]) else {
            bail!("trailing data after final checksum");
        };
        Ok(())
    }
}

impl<R: BufRead> BufRead for HashingReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.reader.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        let bytes = self
            .reader
            .fill_buf()
            .expect("previous call to fill_buf succeeded");
        let amt = std::cmp::min(amt, bytes.len());
        self.hasher.update(&bytes[..amt]);
        self.reader.consume(amt);
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.reader.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

enum DeltaType {
    OfsDelta,
    RefDelta,
}

enum PackObjType {
    Basic(ObjType),
    Delta(DeltaType),
}
use PackObjType::*;

impl PackObjType {
    fn from_byte(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Basic(ObjType::Commit)),
            2 => Ok(Basic(ObjType::Tree)),
            3 => Ok(Basic(ObjType::Blob)),
            4 => Ok(Basic(ObjType::Tag)),
            6 => Ok(Delta(DeltaType::OfsDelta)),
            7 => Ok(Delta(DeltaType::RefDelta)),
            _ => bail!("unknow pack object type: {}", value),
        }
    }
}

fn unpack_undeltified(reader: &mut impl BufRead, obj_type: ObjType, size: usize) -> Result<()> {
    let mut zdec = ZlibDecoder::new(reader);
    let mut object = ObjWriter::new(obj_type, size, true).context("creating object")?;
    io::copy(&mut zdec, &mut object).context("copying data to object")?;
    object
        .finish()
        .context("writing object to object database")?;
    Ok(())
}

fn unpack_ref_delta(reader: &mut impl BufRead, size: usize) -> Result<()> {
    let mut hash = [0u8; 20];
    reader
        .read_exact(&mut hash)
        .context("reading base of ref_delta entry")?;
    let hash = hex::encode(hash);
    let base_obj =
        ObjReader::from_hash(&hash).with_context(|| format!("opening base object {hash}"))?;
    let writer = ObjWriter::new(base_obj.obj_type, size, true)
        .context("creating new object from ref_delta")?;

    let _ = writer;
    bail!("ref_delta not implemented yet");
}

// gitformat-pack(5) "object entries, each of which looks like this"
fn unpack_object(reader: &mut impl BufRead) -> Result<()> {
    // n-byte type and length (3-bit type, (n-1)*7+4-bit length)
    let mut buf = [0u8];
    reader
        .read_exact(&mut buf)
        .context("reading object's first byte")?;
    let type_id = (buf[0] >> 4) & 0b111;
    let pack_type = PackObjType::from_byte(type_id)?;

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
    match pack_type {
        Basic(obj_type) => unpack_undeltified(reader, obj_type, size),
        Delta(DeltaType::RefDelta) => unpack_ref_delta(reader, size),
        Delta(DeltaType::OfsDelta) => bail!("ofs_delta not supported"),
    }
}

// gitformat-pack(5) "pack-*.pack files have the following format"
pub fn unpack_from<R: BufRead>(reader: R) -> Result<u32> {
    let mut reader = HashingReader::new(reader);

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
    reader.finish().context("end of packfile")?;

    Ok(nb_obj)
}
