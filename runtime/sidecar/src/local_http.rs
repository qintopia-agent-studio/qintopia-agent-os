//! Bounded loopback-only HTTP for local workbenches; no outbound client.
use anyhow::{ensure, Result};
use std::collections::BTreeMap;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub(crate) struct Request {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: Vec<u8>,
}
pub(crate) async fn request(stream: &mut TcpStream, port: u16) -> Result<Request> {
    let mut buffer = Vec::new();
    let mut byte = [0_u8; 1];
    while !buffer.ends_with(b"\r\n\r\n") {
        ensure!(buffer.len() < 8192, "header_too_large");
        stream.read_exact(&mut byte).await?;
        buffer.push(byte[0]);
    }
    let header = std::str::from_utf8(&buffer)?;
    let mut lines = header.split("\r\n");
    let first = lines.next().unwrap().split(' ').collect::<Vec<_>>();
    ensure!(
        first.len() == 3 && first[2] == "HTTP/1.1",
        "http_version_required"
    );
    let mut headers = BTreeMap::new();
    for line in lines.filter(|s| !s.is_empty()) {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid_header"))?;
        ensure!(
            !name.is_empty() && !name.contains(char::is_whitespace),
            "invalid_header_name"
        );
        ensure!(
            headers
                .insert(name.to_ascii_lowercase(), value.trim().to_string())
                .is_none(),
            "duplicate_header"
        );
    }
    ensure!(
        headers.get("host") == Some(&format!("127.0.0.1:{port}")),
        "host_forbidden"
    );
    ensure!(
        !headers.contains_key("transfer-encoding") && !headers.contains_key("content-encoding"),
        "encoded_body_forbidden"
    );
    let length = headers
        .get("content-length")
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    ensure!(length <= 65536, "body_too_large");
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await?;
    Ok(Request {
        method: first[0].into(),
        path: first[1].into(),
        headers,
        body,
    })
}

pub(crate) async fn respond(
    stream: &mut TcpStream,
    status: u16,
    mime: &str,
    body: &[u8],
    cookie: Option<(&str, &str)>,
) -> Result<()> {
    let cookie = cookie
        .map(|(name, value)| {
            format!("Set-Cookie: {name}={value}; HttpOnly; SameSite=Strict; Path=/\r\n")
        })
        .unwrap_or_default();
    stream.write_all(format!("HTTP/1.1 {status} Response\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; frame-ancestors 'none'\r\n{cookie}Connection: close\r\n\r\n",body.len()).as_bytes()).await?;
    stream.write_all(body).await?;
    Ok(())
}
