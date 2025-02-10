use anyhow::{bail, Context, Result};
use flate2::{write::ZlibEncoder, Compression};
use rand::Rng;
use sha1::{Digest, Sha1};
use std::fs;
use std::io;
use std::io::prelude::*;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::common::*;
use crate::obj_type::ObjType;

pub struct ObjWriter {
    hasher: Sha1,
    zenc: Option<ZlibEncoder<fs::File>>,
    size: usize,
    seen: usize,
    past_header: bool,
    tmp_rand: [u8; 20],
}

impl ObjWriter {
    fn tmp_path(tmp_rand: &[u8]) -> Result<PathBuf> {
        let tmp_name = format!("tmpobj{}", hex::encode(tmp_rand));
        Ok(git_dir()?.join(tmp_name))
    }

    pub fn new(obj_type: ObjType, size: usize, write: bool) -> Result<ObjWriter> {
        let hasher = Sha1::new();

        let mut tmp_rand = [0u8; 20];
        if write {
            rand::rng().fill(&mut tmp_rand);
        }
        let zenc = if write {
            // We don't know the name (hash) yet, so use a temporary file
            let tmp_path = Self::tmp_path(&tmp_rand)?;
            let file = fs::File::create(&tmp_path)
                .with_context(|| format!("could not create {}", tmp_path.display()))?;

            let mut perms = file.metadata()?.permissions();
            perms.set_readonly(true);
            fs::set_permissions(tmp_path, perms)?;

            Some(ZlibEncoder::new(file, Compression::default()))
        } else {
            None
        };

        let mut writer = ObjWriter {
            hasher,
            zenc,
            size,
            seen: 0,
            past_header: false,
            tmp_rand,
        };
        write!(writer, "{} {}\0", obj_type.to_str(), size).context("writing object header")?;
        writer.past_header = true;
        Ok(writer)
    }

    pub fn finish(self) -> Result<String> {
        if self.seen != self.size {
            bail!("size mismatch: expected {}, got {}", self.size, self.seen);
        }

        let hash_bin = self.hasher.finalize();
        let hash_hex = format!("{:x}", hash_bin);

        if let Some(zenc) = self.zenc {
            zenc.finish().context("closing zlib stream")?;
            let from = Self::tmp_path(&self.tmp_rand)?;
            let to = path_from_hash(&hash_hex)?;
            fs::create_dir_all(to.parent().unwrap())
                .with_context(|| format!("creating {}", to.parent().unwrap().display()))?;
            fs::rename(from, &to)
                .with_context(|| format!("renaming temporary file to {}", to.display()))?;
        }

        Ok(hash_hex)
    }
}

impl Write for ObjWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = if let Some(zenc) = &mut self.zenc {
            zenc.write(buf)?
        } else {
            buf.len()
        };

        self.hasher.update(&buf[..n]);
        if self.past_header {
            self.seen += n;
        }

        if self.seen > self.size {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("size mismatch: expected {}, got {}+", self.size, self.seen),
            ))
        } else {
            Ok(n)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.zenc {
            Some(z) => z.flush(),
            None => Ok(()),
        }
    }
}

pub fn write_object<R>(obj_type: ObjType, source: &mut R, write: bool) -> Result<String>
where
    R: Read + Seek,
{
    let current_pos = source.stream_position()?;
    let size = source.seek(SeekFrom::End(0))?;
    source.seek(SeekFrom::Start(current_pos))?;

    let size = usize::try_from(size).context("object size does no fit in usize")?;

    let mut object = ObjWriter::new(obj_type, size, write)?;
    io::copy(source, &mut object).context("copying to object")?;
    object.finish()
}
