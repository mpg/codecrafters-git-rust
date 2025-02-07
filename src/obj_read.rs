use anyhow::{ensure, Context, Result};
use flate2::bufread::ZlibDecoder;
use std::fs;
use std::io;
use std::io::prelude::*;

use crate::common::*;
use crate::obj_type::ObjType;

// Read the size field of a loose object: ascii digits terminated by '\0'.
fn read_obj_size(s: &mut impl Read) -> Result<usize> {
    let digits = read_up_to(s, b'\0')?;
    let size: usize = std::str::from_utf8(&digits)?.parse()?;
    Ok(size)
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

        let obj_type = ObjType::from_stream(&mut zdec)
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
}

impl Read for ObjReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.zdec.read(buf) {
            Ok(0) => {
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
