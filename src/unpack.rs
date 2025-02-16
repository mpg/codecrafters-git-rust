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

fn read_byte(reader: &mut impl Read) -> Result<u8> {
    let mut buf = [0u8];
    reader.read_exact(&mut buf)?;
    Ok(buf[0])
}

// gitformat-pack(5) "Size encoding",
// optionally keeping type_bits from the first byte to encode a type ID.
fn read_size_and_opt_type(reader: &mut impl Read, type_bits: u8) -> Result<(u8, usize)> {
    let mut byte = read_byte(reader).context("reading first byte")?;

    // split the first byte between size and type
    let mut size_bits = 7 - type_bits;
    let size_mask = (1 << size_bits) - 1;
    let type_mask = 0x7f & !size_mask;
    let type_id = (byte & type_mask) >> size_bits;
    let mut size = (byte & size_mask) as usize;

    // keep reading size until the top bit is unset
    while byte & 0x80 != 0 {
        byte = read_byte(reader).context("reading continuation byte")?;
        size += ((byte & 0x7f) as usize) << size_bits;
        size_bits += 7;
    }

    Ok((type_id, size))
}

// gitformat-pack(5) "Instruction to copy from base object" - offset
fn read_copy_offset(reader: &mut impl Read, bitmap: u8) -> Result<usize> {
    let mut offset = 0;
    for b in 0..4 {
        if bitmap & (1 << b) != 0 {
            let byte = read_byte(reader).context("read next byte")?;
            offset += (byte as usize) << (8 * b);
        }
    }
    Ok(offset)
}

// gitformat-pack(5) "Instruction to copy from base object" - size
fn read_copy_size(reader: &mut impl Read, bitmap: u8) -> Result<usize> {
    let mut size = 0;
    for b in 0..3 {
        if bitmap & (1 << (4 + b)) != 0 {
            let byte = read_byte(reader).context("read next byte")?;
            size += (byte as usize) << (8 * b);
        }
    }
    Ok(size)
}

fn unpack_ref_delta(reader: &mut impl BufRead, instr_size: usize) -> Result<()> {
    let mut hash = [0u8; 20];
    reader
        .read_exact(&mut hash)
        .context("reading hash of base object")?;
    let hash = hex::encode(hash);

    let mut reader = &mut ZlibDecoder::new(reader);
    let (_, base_size) = read_size_and_opt_type(&mut reader, 0).context("reading base size")?;
    let (_, obj_size) = read_size_and_opt_type(&mut reader, 0).context("reading object size")?;

    let mut base_obj =
        ObjReader::from_hash(&hash).with_context(|| format!("opening base object {hash}"))?;
    debug_assert!(base_obj.size == base_size);
    // For now, read base object all at once in memory
    let mut base = Vec::with_capacity(base_size);
    io::copy(&mut base_obj, &mut base).with_context(|| format!("reading base object {hash}"))?;

    let mut writer = ObjWriter::new(base_obj.obj_type, obj_size, true)
        .context("creating new object from ref_delta")?;

    while reader.total_out() < instr_size as u64 {
        let first_byte = read_byte(reader).context("reading next instruction")?;
        if first_byte & 0x80 != 0 {
            let offset = read_copy_offset(reader, first_byte).context("reading offset")?;
            let copy_size = read_copy_size(reader, first_byte).context("reading size")?;
            writer
                .write_all(&base[offset..offset + copy_size])
                .context("copy from base")?;
        } else {
            let add_size = first_byte as usize;
            let mut buf = vec![0u8; add_size];
            reader
                .read_exact(&mut buf)
                .context("reading 'add new data' data")?;
            writer
                .write_all(&buf)
                .context("writing 'add new data' data")?;
        }
    }

    writer.finish().context("finalizing object")?;

    Ok(())
}

// gitformat-pack(5) "object entries, each of which looks like this"
fn unpack_object(reader: &mut impl BufRead) -> Result<()> {
    // n-byte type and length (3-bit type, (n-1)*7+4-bit length)
    let (type_id, size) = read_size_and_opt_type(reader, 3).context("reading type and size")?;
    let pack_type = PackObjType::from_byte(type_id)?;

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
