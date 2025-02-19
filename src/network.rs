use anyhow::{Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue};
use std::io::prelude::*;

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
