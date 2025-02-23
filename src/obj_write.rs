//! Write objects to loose storage, and compute their hash.

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

/// Generic object writer/hasher: data can be provided in a streaming way
/// using the Write trait, but total size needs to be known upfront.
///
/// Can either just compute the object hash, or also write it to the filesystem.
pub struct ObjWriter {
    hasher: Sha1,
    zenc: Option<ZlibEncoder<fs::File>>,
    size: usize,
    seen: usize,
    past_header: bool,
    tmp_rand: [u8; 20],
}

impl ObjWriter {
    /// Pick a name for the temporary file.
    ///
    /// We can't write directly to the final location, as it is determined by
    /// the hash of the header+content which won't be known until the end.
    fn tmp_path(tmp_rand: &[u8]) -> Result<PathBuf> {
        let tmp_name = format!("tmpobj{}", hex::encode(tmp_rand));
        Ok(git_dir()?.join(tmp_name))
    }

    /// Create an object writer.
    ///
    /// Immediately handle the header, and get ready to receive content.
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

            // Mimick git and set the file read-only.
            let mut perms = file.metadata()?.permissions();
            perms.set_readonly(true);
            fs::set_permissions(tmp_path, perms).context("making temporary file read-only")?;

            Some(ZlibEncoder::new(file, Compression::default()))
        } else {
            None
        };

        // Object format: <type> <size>\0<content>, all zlib-compressed
        // The hash is over all of the above (header+content) before compression.
        // The size is that the contents (excluding the header).
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

    /// Finalize object creation. Call this when all data has been written,
    /// to get the object's hash (and write it to permanent storage if selected).
    ///
    /// Checks that the size of the data written matches the announced size.
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
            fs::create_dir_all(to.parent().expect("object path has a parent"))
                .with_context(|| format!("creating {}", to.parent().unwrap().display()))?;
            fs::rename(from, &to)
                .with_context(|| format!("renaming temporary file to {}", to.display()))?;
        }

        Ok(hash_hex)
    }
}

impl Write for ObjWriter {
    /// Writes a buffer into this object, returning how many bytes were written.
    ///
    /// Ensure we don't write more than the announced size.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = match &mut self.zenc {
            Some(zenc) => zenc.write(buf)?,
            None => buf.len(),
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

    /// Flushes this output stream,
    /// ensuring that all intermediately buffered contents reach their destination.
    fn flush(&mut self) -> io::Result<()> {
        match &mut self.zenc {
            Some(z) => z.flush(),
            None => Ok(()),
        }
    }
}

/// Write an object using data from a Reader.
///
/// We require the source to implement Seek as we need to know the size in advance.
/// In practice the two sources we'll use are File and Cursor.
pub fn write_object<R>(obj_type: ObjType, source: &mut R, write: bool) -> Result<String>
where
    R: Read + Seek,
{
    let current_pos = source.stream_position()?;
    let size = source.seek(SeekFrom::End(0))?;
    source.seek(SeekFrom::Start(current_pos))?;

    let size = usize::try_from(size).context("object size does no fit in usize")?;

    let mut object = ObjWriter::new(obj_type, size, write).context("creating object")?;
    io::copy(source, &mut object).context("copying to object")?;
    object.finish()
}
