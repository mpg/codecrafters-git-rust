//! The subset of the Git v2 protocol (over HTTP) that is used to minimally clone.
//!
//! References:
//! - gitprotocol-common(5) <https://git-scm.com/docs/gitprotocol-common>
//! - gitprotocol-v2(5) <https://git-scm.com/docs/gitprotocol-v2>
//!
//! Note: compared to the documentation, we skip the discovery phase,
//! and just assume the server implements the smart HTTP protocol v2.

use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue};
use std::io;
use std::io::prelude::*;
use std::str;

fn io_err_invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

/// Read the length of a packet line, see gitprotocol-common(5) "pkt-line Format".
/// Return the length of the following data (excluding the length bytes).
///
/// Note: there are more than one special packet (for example 0001 is delimiter),
/// so in principle with should use a dedicated enum. But since we only need one,
/// we use a simple Option with None representing flush-pkt.
fn read_pkt_line_len(src: &mut impl Read) -> io::Result<Option<usize>> {
    let mut buf = [0; 4];
    src.read_exact(&mut buf)?;
    let Ok(len) = str::from_utf8(&buf) else {
        return Err(io_err_invalid("invalid pkt-line length: not UTF-8"));
    };
    let Ok(len) = usize::from_str_radix(len, 16) else {
        return Err(io_err_invalid("invalid pkt-line length: not hex"));
    };

    if len == 0 {
        return Ok(None);
    }

    if len < 4 {
        return Err(io_err_invalid(&format!("invalid pkt-line length: {}", len)));
    }
    let len = len - 4;

    Ok(Some(len))
}

/// Filter wrapping a Response to a fetch request and returning the bytes of the packfile.
///
/// The response to the fetch request is in pkt-line format, with the first line
/// indicating a packfile, and the following lines divided into multiple streams:
/// - channel 1 is the packfile;
/// - channel 2 would be progress info, but see below;
/// - channel 3 is for errors.
///
/// Assume no-progress has been used in the request, so we only read from channel #1
/// and treat everything else as a fatal error.
///
/// This reader checks the first line, and returns the content from channel #1,
/// until the first flush-pkt, signaling EOF.
///
/// It implements BufRead for the benefit of the zlib decompressor in the unpack module.
struct PackFileReader {
    /// Internal buffer
    buf: Vec<u8>,
    /// Position of the next byte to return in the buffer
    pos: usize,
    /// Number of valid bytes in the buffer
    cap: usize,
    /// Remaining bytes in the current pkt-line
    rem: usize,
    /// Internal reader
    src: Response,
}

impl PackFileReader {
    /// Create a packfile reader from a Response to a fetch request (with no-progress).
    fn new(mut resp: Response) -> Result<Self> {
        let mut buf = vec![0u8; 8192];

        let len = read_pkt_line_len(&mut resp)
            .context("reading first pkt-line")?
            .context("first pkt-line was a flush")?;

        // Check the first line: we expect "packfile" or "packfile\n".
        if len > buf.len() {
            bail!("overly long first pkt-line, exp. 9 got {}", len);
        }

        resp.read_exact(&mut buf[..len])?;

        let len = if buf[len - 1] == b'\n' { len - 1 } else { len };
        if &buf[..len] != b"packfile" {
            bail!("expected 'packfile', got '{:?}'", &buf[..len]);
        }

        Ok(Self {
            buf,
            pos: 0,
            cap: 0,
            rem: 0,
            src: resp,
        })
    }
}

impl BufRead for PackFileReader {
    /// Returns the contents of the internal buffer,
    /// filling it with more data from the inner reader if it is empty.
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        // Fill the buffer if empty. A loop is needed in case a pkt-line is empty.
        while self.pos >= self.cap {
            debug_assert!(self.pos == self.cap);

            if self.rem != 0 {
                // Read next bytes form current pkt-line
                let len = std::cmp::min(self.buf.len(), self.rem);
                self.src.read_exact(&mut self.buf[..len])?;

                self.pos = 0;
                self.cap = len;
                self.rem -= len;

                break;
            }

            // Start a new pkt-line
            let line_len = read_pkt_line_len(&mut self.src)?;
            let Some(line_len) = line_len else {
                // Flush means EOF, which we signal with empty slice
                return Ok(&[]);
            };

            let use_len = std::cmp::min(self.buf.len(), line_len);
            self.src.read_exact(&mut self.buf[..use_len])?;

            // We only expect data from channel #1
            if line_len < 1 {
                return Err(io_err_invalid("next pkt-line has no channel ID"));
            }

            if self.buf[0] != 1 {
                return Err(io_err_invalid(&format!(
                    "unexpected channel ID: {}",
                    self.buf[0]
                )));
            }

            self.pos = 1;
            self.cap = use_len;
            self.rem = line_len - use_len;
        }

        Ok(&self.buf[self.pos..self.cap])
    }

    /// Tells this buffer that amt bytes have been consumed from the buffer,
    /// so they should no longer be returned in calls to read.
    fn consume(&mut self, amt: usize) {
        self.pos += amt;
    }
}

impl Read for PackFileReader {
    /// Pull some bytes from this source into the specified buffer,
    /// returning how many bytes were read.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let data = self.fill_buf()?;
        let len = std::cmp::min(buf.len(), data.len());
        buf[..len].copy_from_slice(&data[..len]);
        self.consume(len);

        Ok(len)
    }
}

/// Make a request to the git-upload-pack service of protocol v2.
pub fn request_upload_pack_v2(repo_url: &str, body: &str) -> Result<Response> {
    let request_url = format!("{}/git-upload-pack", repo_url.trim_end_matches('/'));

    let mut headers = HeaderMap::new();
    headers.insert("git-protocol", HeaderValue::from_static("version=2"));

    let response = Client::new()
        .post(request_url)
        .headers(headers)
        .body(body.to_owned())
        .send()
        .context("sending request to server")?;
    Ok(response)
}

/// Make a ls-refs request and return:
/// - the hash of the remote HEAD;
/// - the name of the default branch.
pub fn ls_remote_head(repo_url: &str) -> Result<(String, String)> {
    // gitprotocol-v2(5) "ls-refs" for the content;
    // gitprotocol-common(5) for pkt-line format.
    //
    // 0013command=ls-refs - list references
    // 0001 - delim-pkt
    // 000bsymrefs - to get the name of the branch pointing to HEAD
    // 0013ref-prefix HEAD - to only get info about HEAD
    // 0000 - flush-pkt
    let body = "0013command=ls-refs0001000bsymrefs0013ref-prefix HEAD0000";
    let mut response = request_upload_pack_v2(repo_url, body).context("making ls-refs request")?;

    let len = read_pkt_line_len(&mut response)
        .context("reading first pkt-line length")?
        .context("unexpected flush at start of response")?;
    let mut line = vec![0; len];
    response
        .read_exact(&mut line)
        .context("reading first pkt-line content")?;
    let line = str::from_utf8(&line).context("response is not ASCII")?;
    let line = line.trim_end_matches('\n');

    // <40-char hash> HEAD symref-target:refs/heads/<name>
    // let's be lazy and directly index into the line
    let hash = line[..40].to_owned();
    let middle = &line[40..71];
    let name = line[71..].to_owned();

    if middle != " HEAD symref-target:refs/heads/" {
        bail!("unsupported response format: {middle}");
    }

    Ok((hash, name))
}

/// Make a fetch request and return a BufRead for the packfile data.
pub fn get_pack(repo_url: &str, head: &str) -> Result<impl BufRead> {
    // gitprotocol-v2(5) "fetch" for the content;
    // gitprotocol-common(5) for pkt-line format.
    //
    // 0011command=fetch
    // 0001 - delim-pkt
    // 000fno-progress - to only receive on side-band channel #1
    // 0031want <hash> - the commit(s) we want
    // 0000 - flush-pkt
    let body = format!("0011command=fetch0001000fno-progress0031want {head}0000");
    let response = request_upload_pack_v2(repo_url, &body).context("making fetch request")?;
    let reader = PackFileReader::new(response).context("parsing fetch response")?;
    Ok(reader)
}
