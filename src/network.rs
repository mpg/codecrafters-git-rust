use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue};
use std::io;
use std::io::prelude::*;
use std::str;

fn io_err_invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

// See gitprotocol-common(5) "pkt-line Format"
fn read_pkt_line(buf: &mut [u8], src: &mut impl Read) -> io::Result<Option<usize>> {
    src.read_exact(&mut buf[..4])?;
    let Ok(len) = str::from_utf8(&buf[..4]) else {
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
    src.read_exact(&mut buf[..len])?;

    Ok(Some(len))
}

// A filter that wraps a Response to a fetch request and returns the bytes of the packfile.
// Assumes no-progress was used in the request so we only receive on stream #1.
struct PackFileReader {
    buf: Vec<u8>,
    pos: usize,
    cap: usize,
    src: Response,
}

impl PackFileReader {
    fn new(mut resp: Response) -> Result<Self> {
        // allocate a buffer large enough for any pkt-line
        let mut buf = vec![0u8; 1 << 16];
        let len = read_pkt_line(&mut buf, &mut resp).context("reading first pkt-line")?;
        let len = len.context("first pkt-line was a flush")?;

        let len = if buf[len - 1] == b'\n' { len - 1 } else { len };
        if &buf[..len] != b"packfile" {
            bail!("expected 'packfile', got '{:?}'", &buf[..len]);
        }

        Ok(Self {
            buf,
            pos: 0,
            cap: 0,
            src: resp,
        })
    }
}

impl BufRead for PackFileReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        while self.pos >= self.cap {
            debug_assert!(self.pos == self.cap);

            let len = read_pkt_line(&mut self.buf, &mut self.src)?;

            let Some(len) = len else {
                // Flush means EOF, which we signal with empty slice
                return Ok(&[]);
            };

            if len < 1 {
                return Err(io_err_invalid("next pkt-line has no stream ID"));
            }

            if self.buf[0] != 1 {
                return Err(io_err_invalid(&format!(
                    "unexpected stream ID: {}",
                    self.buf[0]
                )));
            }

            self.pos = 1;
            self.cap = len;
        }

        Ok(&self.buf[self.pos..self.cap])
    }

    fn consume(&mut self, amt: usize) {
        self.pos += amt;
    }
}

impl Read for PackFileReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let data = self.fill_buf()?;
        let len = std::cmp::min(buf.len(), data.len());
        buf[..len].copy_from_slice(&data[..len]);
        self.consume(len);

        Ok(len)
    }
}

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

pub fn ls_remote_head(repo_url: &str) -> Result<String> {
    // gitprotocol-v2(5) + gitprotocol-common(5)
    let body = "0013command=ls-refs0000";
    let mut response =
        request_upload_pack_v2(repo_url, body).context("making ls-remote request")?;
    let mut data = String::new();
    response
        .read_to_string(&mut data)
        .context("reading response from server")?;

    // we know HEAD is the first line, and hash comes after 4-digit length
    Ok(data[4..44].to_owned())
}

pub fn get_pack(repo_url: &str, head: &str) -> Result<impl BufRead> {
    // gitprotocol-v2(5) "fetch"
    let body = format!("0011command=fetch0001000fno-progress0031want {head}0000");
    let response = request_upload_pack_v2(repo_url, &body).context("making fetch request")?;
    let reader = PackFileReader::new(response).context("parsing fetch response")?;
    Ok(reader)
}
