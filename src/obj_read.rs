use anyhow::{anyhow, ensure, Context, Result};
use flate2::bufread::ZlibDecoder;
use std::fs;
use std::io;
use std::io::prelude::*;

use crate::common::*;
use crate::obj_type::ObjType;

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

// Read the size field of a loose object: ascii digits terminated by '\0'.
fn read_obj_size(s: &mut impl Read) -> Result<usize> {
    let digits = read_up_to(s, b'\0')?;
    let size: usize = std::str::from_utf8(&digits)?.parse()?;
    Ok(size)
}

// Read the type of a loose object: ascii string terminated by space.
fn read_obj_type(s: &mut impl Read) -> Result<ObjType> {
    let label = read_up_to(s, b' ')?;
    ObjType::from_bytes(&label)
}

pub struct ObjReader {
    pub obj_type: ObjType,
    pub size: usize,
    used: usize,
    zdec: ZlibDecoder<io::BufReader<fs::File>>,
}

impl ObjReader {
    pub fn from_hash(hash: &str) -> Result<ObjReader> {
        ensure!(hash.len() >= 4, "not a valid object name {}", hash);
        let obj_path = path_from_hash(hash)?;

        let file = fs::File::open(obj_path)
            .with_context(|| format!("not a valid object name {}", hash))?;
        let bufreader = io::BufReader::new(file);
        let mut zdec = ZlibDecoder::new(bufreader);

        let obj_type = read_obj_type(&mut zdec)
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

    pub fn read_up_to(&mut self, delim: u8) -> Result<Vec<u8>> {
        read_up_to(self, delim)
    }

    // Tell if EOF has been reached,
    // without consuming bytes unless the object reaches an error state
    pub fn eof(&mut self) -> Result<bool> {
        if self.used < self.size {
            Ok(false)
        } else {
            match self.zdec.read(&mut [0]) {
                Ok(0) => Ok(true),
                Err(e) => Err(e.into()),
                _ => {
                    self.used = self.size + 1;
                    Err(anyhow!("size mismatch: trailing bytes present"))
                }
            }
        }
    }
}

impl Read for ObjReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.zdec.read(buf) {
            Ok(0) if !buf.is_empty() => {
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
