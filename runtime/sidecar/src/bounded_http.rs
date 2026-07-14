use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use rustls::{ClientConfig, ClientConnection, OwnedTrustAnchor, RootCertStore, ServerName, Stream};
use url::Url;
use zeroize::{Zeroize, Zeroizing};

pub(crate) const MAX_HTTP_RESPONSE_HEADER_BYTES: usize = 64 * 1024;
const DEFAULT_SOCKET_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub(crate) struct HttpClient {
    tls_config: Arc<ClientConfig>,
    allow_insecure_http: bool,
    socket_timeout: Duration,
}

pub(crate) struct HttpResponse {
    pub(crate) status: u16,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: Vec<u8>,
}

impl Drop for HttpResponse {
    fn drop(&mut self) {
        self.body.zeroize();
        for value in self.headers.values_mut() {
            value.zeroize();
        }
    }
}

#[derive(Debug)]
pub(crate) struct HttpRequestError {
    pub(crate) transport: bool,
    #[cfg(any(test, feature = "qiwe-staging-adapter"))]
    request_may_have_been_sent: bool,
    source: anyhow::Error,
}

type HttpRequestResult<T> = std::result::Result<T, HttpRequestError>;

impl HttpRequestError {
    fn transport(source: anyhow::Error) -> Self {
        Self {
            transport: true,
            #[cfg(any(test, feature = "qiwe-staging-adapter"))]
            request_may_have_been_sent: false,
            source,
        }
    }

    fn terminal(source: anyhow::Error) -> Self {
        Self {
            transport: false,
            #[cfg(any(test, feature = "qiwe-staging-adapter"))]
            request_may_have_been_sent: false,
            source,
        }
    }

    fn after_send(source: anyhow::Error, transport: bool) -> Self {
        Self {
            transport,
            #[cfg(any(test, feature = "qiwe-staging-adapter"))]
            request_may_have_been_sent: true,
            source,
        }
    }

    pub(crate) fn into_source(self) -> anyhow::Error {
        self.source
    }

    #[cfg(any(test, feature = "qiwe-staging-adapter"))]
    pub(crate) fn request_may_have_been_sent(&self) -> bool {
        self.request_may_have_been_sent
    }
}

impl std::fmt::Display for HttpRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(formatter)
    }
}

impl HttpClient {
    pub(crate) fn production() -> Self {
        Self {
            tls_config: Arc::new(tls_config()),
            allow_insecure_http: false,
            socket_timeout: DEFAULT_SOCKET_TIMEOUT,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_only() -> Self {
        Self::test_only_with_timeout(DEFAULT_SOCKET_TIMEOUT)
    }

    #[cfg(test)]
    pub(crate) fn test_only_with_timeout(socket_timeout: Duration) -> Self {
        Self {
            tls_config: Arc::new(tls_config()),
            allow_insecure_http: true,
            socket_timeout,
        }
    }

    pub(crate) fn request(
        &self,
        method: &str,
        endpoint: &Url,
        headers: &[(&str, String)],
        body: &[u8],
        max_response_body_bytes: usize,
    ) -> HttpRequestResult<HttpResponse> {
        validate_http_method(method).map_err(HttpRequestError::terminal)?;
        let host = endpoint
            .host_str()
            .ok_or_else(|| anyhow!("HTTP endpoint is missing a host"))
            .map_err(HttpRequestError::terminal)?;
        let port = endpoint
            .port_or_known_default()
            .ok_or_else(|| anyhow!("HTTP endpoint has no known port"))
            .map_err(HttpRequestError::terminal)?;
        let mut path = endpoint.path().to_string();
        if path.is_empty() {
            path = "/".to_string();
        }
        if let Some(query) = endpoint.query() {
            path.push('?');
            path.push_str(query);
        }
        for (name, value) in headers {
            validate_http_header(name, value).map_err(HttpRequestError::terminal)?;
        }
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            request.push_str(name);
            request.push_str(": ");
            request.push_str(value);
            request.push_str("\r\n");
        }
        request.push_str("\r\n");
        let mut request_bytes = Zeroizing::new(request.into_bytes());
        request_bytes.extend_from_slice(body);

        let response = match endpoint.scheme() {
            "https" => {
                let server_name = ServerName::try_from(host)
                    .context("validate HTTPS host")
                    .map_err(HttpRequestError::terminal)?;
                let mut connection =
                    ClientConnection::new(Arc::clone(&self.tls_config), server_name)
                        .context("create adapter TLS connection")
                        .map_err(HttpRequestError::terminal)?;
                let mut socket = TcpStream::connect((host, port))
                    .context("connect adapter endpoint")
                    .map_err(HttpRequestError::transport)?;
                configure_socket(&socket, self.socket_timeout)
                    .map_err(HttpRequestError::transport)?;
                let mut stream = Stream::new(&mut connection, &mut socket);
                stream
                    .write_all(&request_bytes)
                    .map_err(|error| request_io_error_after_send("write adapter request", error))?;
                stream
                    .flush()
                    .map_err(|error| request_io_error_after_send("flush adapter request", error))?;
                read_response_limited(&mut stream, max_response_body_bytes)?
            }
            "http" if self.allow_insecure_http => {
                let mut socket = TcpStream::connect((host, port))
                    .context("connect test adapter endpoint")
                    .map_err(HttpRequestError::transport)?;
                configure_socket(&socket, self.socket_timeout)
                    .map_err(HttpRequestError::transport)?;
                socket.write_all(&request_bytes).map_err(|error| {
                    request_io_error_after_send("write test adapter request", error)
                })?;
                socket.flush().map_err(|error| {
                    request_io_error_after_send("flush test adapter request", error)
                })?;
                read_response_limited(&mut socket, max_response_body_bytes)?
            }
            _ => {
                return Err(HttpRequestError::terminal(anyhow!(
                    "adapter endpoints must use HTTPS"
                )))
            }
        };
        parse_http_response(response, max_response_body_bytes)
            .map_err(|error| HttpRequestError::after_send(error, false))
    }

    pub(crate) fn allows_insecure_http(&self) -> bool {
        self.allow_insecure_http
    }
}

fn validate_http_method(method: &str) -> Result<()> {
    if method.is_empty() || !method.bytes().all(|byte| byte.is_ascii_uppercase()) {
        bail!("adapter request contains an invalid HTTP method");
    }
    Ok(())
}

pub(crate) fn validate_http_header(name: &str, value: &str) -> Result<()> {
    if name.is_empty() || !name.bytes().all(is_http_header_name_byte) {
        bail!("adapter request contains an invalid header name");
    }
    if !value.bytes().all(is_http_header_value_byte) {
        bail!("adapter request contains an invalid header value");
    }
    Ok(())
}

fn is_http_header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn is_http_header_value_byte(byte: u8) -> bool {
    byte == b'\t' || (b' '..=b'~').contains(&byte)
}

fn tls_config() -> ClientConfig {
    let mut roots = RootCertStore::empty();
    roots.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(|anchor| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            anchor.subject,
            anchor.spki,
            anchor.name_constraints,
        )
    }));
    ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

fn configure_socket(socket: &TcpStream, timeout: Duration) -> Result<()> {
    socket
        .set_read_timeout(Some(timeout))
        .context("set adapter read timeout")?;
    socket
        .set_write_timeout(Some(timeout))
        .context("set adapter write timeout")?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn request_io_error(context: &'static str, error: io::Error) -> HttpRequestError {
    request_io_error_with_state(context, error, false)
}

fn request_io_error_after_send(context: &'static str, error: io::Error) -> HttpRequestError {
    request_io_error_with_state(context, error, true)
}

fn request_io_error_with_state(
    context: &'static str,
    error: io::Error,
    request_may_have_been_sent: bool,
) -> HttpRequestError {
    let terminal = matches!(
        error.kind(),
        io::ErrorKind::InvalidData
            | io::ErrorKind::InvalidInput
            | io::ErrorKind::PermissionDenied
            | io::ErrorKind::Unsupported
    );
    let source = anyhow::Error::new(error).context(context);
    if request_may_have_been_sent {
        HttpRequestError::after_send(source, !terminal)
    } else if terminal {
        HttpRequestError::terminal(source)
    } else {
        HttpRequestError::transport(source)
    }
}

pub(crate) fn read_response_limited(
    reader: &mut impl Read,
    max_body_bytes: usize,
) -> HttpRequestResult<Vec<u8>> {
    let max_response_bytes = max_body_bytes
        .checked_add(MAX_HTTP_RESPONSE_HEADER_BYTES)
        .ok_or_else(|| anyhow!("adapter response size limit overflow"))
        .map_err(|error| HttpRequestError::after_send(error, false))?;
    let mut response = Vec::with_capacity(max_response_bytes.min(64 * 1024));
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| request_io_error_after_send("read adapter response", error))?;
        if count == 0 {
            return Ok(response);
        }
        if response.len().saturating_add(count) > max_response_bytes {
            return Err(HttpRequestError::after_send(
                anyhow!("adapter response exceeded the configured size limit"),
                false,
            ));
        }
        response.extend_from_slice(&buffer[..count]);
    }
}

pub(crate) fn parse_http_response(bytes: Vec<u8>, max_body_bytes: usize) -> Result<HttpResponse> {
    let mut bytes = Zeroizing::new(bytes);
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| anyhow!("adapter response is missing headers"))?;
    if header_end.saturating_add(4) > MAX_HTTP_RESPONSE_HEADER_BYTES {
        bail!("adapter response headers exceeded the configured size limit");
    }
    let head = std::str::from_utf8(&bytes[..header_end]).context("decode adapter headers")?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| anyhow!("adapter response is missing a status"))?
        .parse::<u16>()
        .context("parse adapter status")?;
    let headers = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect::<BTreeMap<_, _>>();
    if let Some(content_length) = headers.get("content-length") {
        let content_length = content_length
            .parse::<usize>()
            .context("parse adapter content length")?;
        if content_length > max_body_bytes {
            bail!("adapter response exceeded the configured size limit");
        }
    }
    let mut body = Zeroizing::new(bytes.split_off(header_end + 4));
    let body = if headers
        .get("transfer-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"))
    {
        decode_chunked_body(&body, max_body_bytes)?
    } else {
        if body.len() > max_body_bytes {
            bail!("adapter response exceeded the configured size limit");
        }
        std::mem::take(&mut *body)
    };
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

pub(crate) fn decode_chunked_body(body: &[u8], max_body_bytes: usize) -> Result<Vec<u8>> {
    let mut cursor = 0;
    let mut decoded = Vec::new();
    loop {
        let line_end = body
            .get(cursor..)
            .and_then(|remaining| remaining.windows(2).position(|window| window == b"\r\n"))
            .map(|offset| cursor + offset)
            .ok_or_else(|| anyhow!("chunked adapter response is missing a chunk size"))?;
        let size = std::str::from_utf8(&body[cursor..line_end])
            .context("decode adapter response chunk size")?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim();
        let size = usize::from_str_radix(size, 16).context("parse adapter response chunk size")?;
        cursor = line_end + 2;
        if size == 0 {
            return Ok(decoded);
        }
        let end = cursor
            .checked_add(size)
            .ok_or_else(|| anyhow!("chunked adapter response overflow"))?;
        if end + 2 > body.len() || &body[end..end + 2] != b"\r\n" {
            bail!("chunked adapter response ended early");
        }
        if decoded.len().saturating_add(size) > max_body_bytes {
            bail!("chunked adapter response exceeded the configured size limit");
        }
        decoded.extend_from_slice(&body[cursor..end]);
        cursor = end + 2;
    }
}
