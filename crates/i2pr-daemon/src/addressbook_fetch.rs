//! Bounded content fetch over an explicitly configured local HTTP proxy.
//!
//! This owner has one network capability: connect to a loopback eepProxy
//! and issue HTTP requests through it. It has no direct-origin socket path,
//! resolver, redirect following, decompressor, or ambient proxy discovery.

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use i2pr_addressbook::{
    MAX_SUBSCRIPTION_BODY_BYTES, SubscriptionSource, validate_subscription_url,
    validate_subscription_validator,
};

const MAX_RESPONSE_HEADER_BYTES: usize = 16 * 1024;
const MAX_RESPONSE_BYTES: usize = MAX_RESPONSE_HEADER_BYTES + MAX_SUBSCRIPTION_BODY_BYTES;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum FetchError {
    InvalidRequest,
    Unavailable,
    ResponseOverBound,
    InvalidResponse,
    Redirect,
    Timeout,
}

#[derive(Debug)]
pub(crate) struct FetchResponse {
    pub(crate) status: u16,
    pub(crate) body: Vec<u8>,
    pub(crate) etag: Option<String>,
    pub(crate) last_modified: Option<String>,
}

/// Narrow transport capability reusable by other daemon-owned consumers
/// such as the Proposal 170 signed-news cache. It exposes bounded response
/// bytes and conditional validators, never a socket or redirect policy.
pub(crate) trait BoundedContentFetcher: Send + Sync {
    fn fetch<'a>(
        &'a self,
        url: &'a str,
        etag: Option<&'a str>,
        last_modified: Option<&'a str>,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<FetchResponse, FetchError>> + Send + 'a>>;
}

pub(crate) struct LoopbackProxyFetcher {
    pub(crate) host: String,
    pub(crate) port: u16,
}

impl BoundedContentFetcher for LoopbackProxyFetcher {
    fn fetch<'a>(
        &'a self,
        url: &'a str,
        etag: Option<&'a str>,
        last_modified: Option<&'a str>,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<FetchResponse, FetchError>> + Send + 'a>> {
        Box::pin(async move {
            let previous = SubscriptionSource {
                entries: Default::default(),
                etag: etag.map(str::to_owned),
                last_modified: last_modified.map(str::to_owned),
            };
            fetch_subscription(&self.host, self.port, url, Some(&previous), timeout).await
        })
    }
}

/// Fetches one HTTP or HTTPS URL through a loopback proxy. Redirects and
/// compressed encodings are rejected; requests negotiate identity encoding.
pub(crate) async fn fetch_subscription(
    proxy_host: &str,
    proxy_port: u16,
    url: &str,
    previous: Option<&SubscriptionSource>,
    timeout: Duration,
) -> Result<FetchResponse, FetchError> {
    validate_subscription_url(url).map_err(|_| FetchError::InvalidRequest)?;
    let proxy_ip = proxy_host
        .parse::<IpAddr>()
        .map_err(|_| FetchError::InvalidRequest)?;
    if !proxy_ip.is_loopback() || proxy_port == 0 {
        return Err(FetchError::InvalidRequest);
    }
    let parsed = parse_http_url(url)?;
    tokio::time::timeout(timeout, async move {
        let address = SocketAddr::new(proxy_ip, proxy_port);
        let mut stream = TcpStream::connect(address)
            .await
            .map_err(|_| FetchError::Unavailable)?;
        if parsed.https {
            let connect = format!(
                "CONNECT {} HTTP/1.1\r\nHost: {}\r\nConnection: keep-alive\r\n\r\n",
                parsed.authority, parsed.authority
            );
            stream
                .write_all(connect.as_bytes())
                .await
                .map_err(|_| FetchError::Unavailable)?;
            read_connect_response(&mut stream).await?;
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let tls = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            let server_name = rustls_pki_types::ServerName::try_from(parsed.server_name)
                .map_err(|_| FetchError::InvalidRequest)?;
            let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(tls));
            let stream = connector
                .connect(server_name, stream)
                .await
                .map_err(|_| FetchError::Unavailable)?;
            exchange(
                stream,
                build_request(parsed.authority, &parsed.target, previous, false)?,
            )
            .await
        } else {
            exchange(
                stream,
                build_request(parsed.authority, &parsed.target, previous, true)?,
            )
            .await
        }
    })
    .await
    .map_err(|_| FetchError::Timeout)?
}

struct ParsedUrl<'a> {
    https: bool,
    authority: &'a str,
    target: String,
    server_name: String,
}

fn parse_http_url(url: &str) -> Result<ParsedUrl<'_>, FetchError> {
    let (https, rest) = if url
        .get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://"))
    {
        (false, &url[7..])
    } else if url
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
    {
        (true, &url[8..])
    } else {
        return Err(FetchError::InvalidRequest);
    };
    let end_authority = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end_authority];
    if authority.is_empty()
        || !authority.is_ascii()
        || authority.contains('@')
        || authority.bytes().any(|byte| byte <= 0x20 || byte == 0x7f)
    {
        return Err(FetchError::InvalidRequest);
    }
    let server_name = authority_hostname(authority)?;
    let suffix = &rest[end_authority..];
    let suffix = suffix.split('#').next().unwrap_or_default();
    let target = if suffix.is_empty() {
        "/".to_owned()
    } else if suffix.starts_with('?') {
        format!("/{suffix}")
    } else {
        suffix.to_owned()
    };
    if !target.starts_with('/') || target.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) {
        return Err(FetchError::InvalidRequest);
    }
    Ok(ParsedUrl {
        https,
        authority,
        target,
        server_name,
    })
}

fn authority_hostname(authority: &str) -> Result<String, FetchError> {
    let (hostname, bracketed_ipv6) = if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']').ok_or(FetchError::InvalidRequest)?;
        let hostname = &rest[..end];
        hostname
            .parse::<std::net::Ipv6Addr>()
            .map_err(|_| FetchError::InvalidRequest)?;
        let suffix = &rest[end + 1..];
        if !suffix.is_empty() {
            let port = suffix.strip_prefix(':').ok_or(FetchError::InvalidRequest)?;
            validate_url_port(port)?;
        }
        (hostname, true)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if port.bytes().all(|byte| byte.is_ascii_digit()) {
            validate_url_port(port)?;
            (host, false)
        } else if authority.contains(':') {
            return Err(FetchError::InvalidRequest);
        } else {
            (authority, false)
        }
    } else {
        (authority, false)
    };
    if hostname.is_empty()
        || !hostname.is_ascii()
        || (!bracketed_ipv6 && hostname.contains(':'))
        || hostname.bytes().any(|byte| byte <= 0x20 || byte == 0x7f)
    {
        return Err(FetchError::InvalidRequest);
    }
    Ok(hostname.to_owned())
}

fn validate_url_port(port: &str) -> Result<(), FetchError> {
    let port = port
        .parse::<u16>()
        .map_err(|_| FetchError::InvalidRequest)?;
    if port == 0 {
        return Err(FetchError::InvalidRequest);
    }
    Ok(())
}

fn build_request(
    authority: &str,
    target: &str,
    previous: Option<&SubscriptionSource>,
    absolute_uri: bool,
) -> Result<Vec<u8>, FetchError> {
    let request_target = if absolute_uri {
        format!("http://{authority}{target}")
    } else {
        target.to_owned()
    };
    let mut request = format!(
        "GET {request_target} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\nAccept-Encoding: identity\r\n"
    );
    if let Some(source) = previous {
        if let Some(etag) = &source.etag {
            validate_subscription_validator(etag).map_err(|_| FetchError::InvalidRequest)?;
            request.push_str("If-None-Match: ");
            request.push_str(etag);
            request.push_str("\r\n");
        }
        if let Some(last_modified) = &source.last_modified {
            validate_subscription_validator(last_modified)
                .map_err(|_| FetchError::InvalidRequest)?;
            request.push_str("If-Modified-Since: ");
            request.push_str(last_modified);
            request.push_str("\r\n");
        }
    }
    request.push_str("\r\n");
    Ok(request.into_bytes())
}

async fn exchange<S>(mut stream: S, request: Vec<u8>) -> Result<FetchResponse, FetchError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream
        .write_all(&request)
        .await
        .map_err(|_| FetchError::Unavailable)?;
    stream
        .shutdown()
        .await
        .map_err(|_| FetchError::Unavailable)?;
    let mut bytes = Vec::with_capacity(8192);
    stream
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| FetchError::Unavailable)?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(FetchError::ResponseOverBound);
    }
    parse_response(&bytes)
}

async fn read_connect_response(stream: &mut TcpStream) -> Result<(), FetchError> {
    let mut header = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    while header.len() < MAX_RESPONSE_HEADER_BYTES {
        stream
            .read_exact(&mut byte)
            .await
            .map_err(|_| FetchError::Unavailable)?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    if !header.ends_with(b"\r\n\r\n") {
        return Err(FetchError::ResponseOverBound);
    }
    let text = core::str::from_utf8(&header).map_err(|_| FetchError::InvalidResponse)?;
    let mut parts = text.split_ascii_whitespace();
    if !matches!(parts.next(), Some("HTTP/1.0" | "HTTP/1.1")) {
        return Err(FetchError::InvalidResponse);
    }
    let status = parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or(FetchError::InvalidResponse)?;
    if status != 200 {
        return Err(FetchError::Unavailable);
    }
    Ok(())
}

fn parse_response(bytes: &[u8]) -> Result<FetchResponse, FetchError> {
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(FetchError::InvalidResponse)?;
    if header_end + 4 > MAX_RESPONSE_HEADER_BYTES {
        return Err(FetchError::ResponseOverBound);
    }
    let headers =
        core::str::from_utf8(&bytes[..header_end]).map_err(|_| FetchError::InvalidResponse)?;
    let mut lines = headers.split("\r\n");
    let status_line = lines.next().ok_or(FetchError::InvalidResponse)?;
    let mut status_parts = status_line.split_ascii_whitespace();
    if !matches!(status_parts.next(), Some("HTTP/1.0" | "HTTP/1.1")) {
        return Err(FetchError::InvalidResponse);
    }
    let status = status_parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or(FetchError::InvalidResponse)?;
    let mut content_length = None;
    let mut content_encoding = None;
    let mut transfer_encoding = None;
    let mut etag = None;
    let mut last_modified = None;
    let mut location = false;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(FetchError::InvalidResponse)?;
        if name.is_empty()
            || !name.bytes().all(|byte| {
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
            })
        {
            return Err(FetchError::InvalidResponse);
        }
        let value = value.trim_matches([' ', '\t']);
        if value.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
            return Err(FetchError::InvalidResponse);
        }
        match name.to_ascii_lowercase().as_str() {
            "content-length" => {
                if content_length.is_some() {
                    return Err(FetchError::InvalidResponse);
                }
                content_length = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| FetchError::InvalidResponse)?,
                );
            }
            "content-encoding" => {
                if content_encoding.is_some() {
                    return Err(FetchError::InvalidResponse);
                }
                content_encoding = Some(value.to_ascii_lowercase());
            }
            "transfer-encoding" => {
                if transfer_encoding.is_some() {
                    return Err(FetchError::InvalidResponse);
                }
                transfer_encoding = Some(value.to_ascii_lowercase());
            }
            "etag" => {
                if etag.is_some() {
                    return Err(FetchError::InvalidResponse);
                }
                etag = Some(value.to_owned());
            }
            "last-modified" => {
                if last_modified.is_some() {
                    return Err(FetchError::InvalidResponse);
                }
                last_modified = Some(value.to_owned());
            }
            "location" => {
                if location {
                    return Err(FetchError::InvalidResponse);
                }
                location = true;
            }
            // Other bounded, syntactically valid headers do not affect
            // framing or cache semantics and are ignored.
            _ => {}
        }
    }
    if location || matches!(status, 301 | 302 | 303 | 307 | 308) {
        return Err(FetchError::Redirect);
    }
    if content_encoding
        .as_deref()
        .is_some_and(|encoding| encoding != "identity")
        || transfer_encoding
            .as_deref()
            .is_some_and(|encoding| encoding != "identity")
    {
        return Err(FetchError::InvalidResponse);
    }
    let body = &bytes[header_end + 4..];
    if status == 304 {
        if !body.is_empty() || content_length.is_some_and(|length| length != 0) {
            return Err(FetchError::InvalidResponse);
        }
    } else if status == 200 {
        let length = content_length.ok_or(FetchError::InvalidResponse)?;
        if length > MAX_SUBSCRIPTION_BODY_BYTES {
            return Err(FetchError::ResponseOverBound);
        }
        if body.len() != length {
            return Err(FetchError::InvalidResponse);
        }
    } else {
        return Err(FetchError::InvalidResponse);
    }
    if let Some(value) = &etag {
        validate_subscription_validator(value).map_err(|_| FetchError::InvalidResponse)?;
    }
    if let Some(value) = &last_modified {
        validate_subscription_validator(value).map_err(|_| FetchError::InvalidResponse)?;
    }
    Ok(FetchResponse {
        status,
        body: body.to_vec(),
        etag,
        last_modified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_success_and_not_modified_responses() {
        let response =
            parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nETag: \"v1\"\r\n\r\nabc")
                .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"abc");
        assert_eq!(response.etag.as_deref(), Some("\"v1\""));
        let not_modified = parse_response(b"HTTP/1.1 304 Not Modified\r\n\r\n").unwrap();
        assert_eq!(not_modified.status, 304);
        assert!(not_modified.body.is_empty());
    }

    #[test]
    fn rejects_redirects_chunking_compression_and_oversized_body() {
        for response in [
            b"HTTP/1.1 302 Found\r\nLocation: http://other.i2p/\r\nContent-Length: 0\r\n\r\n"
                .as_slice(),
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Encoding: gzip\r\n\r\nx",
        ] {
            assert!(parse_response(response).is_err());
        }
        let oversized = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            MAX_SUBSCRIPTION_BODY_BYTES + 1
        );
        assert_eq!(
            parse_response(oversized.as_bytes()).unwrap_err(),
            FetchError::ResponseOverBound
        );
    }

    #[test]
    fn accepts_only_explicit_loopback_http_proxy_and_safe_targets() {
        assert!(parse_http_url("http://example.i2p/hosts.txt?x=1").is_ok());
        assert!(
            parse_http_url("https://example.i2p/hosts.txt")
                .unwrap()
                .https
        );
        assert!(parse_http_url("http://user@example.i2p/hosts.txt").is_err());
        assert_eq!(
            parse_http_url("http://example.i2p?x=1").unwrap().target,
            "/?x=1"
        );
        assert!(!"192.0.2.1".parse::<IpAddr>().unwrap().is_loopback());
    }

    #[tokio::test]
    async fn local_proxy_receives_conditional_request_and_returns_304() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let proxy = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 1024];
            loop {
                let count = stream.read(&mut chunk).await.unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..count]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("GET http://lists.i2p/hosts.txt HTTP/1.1\r\n"));
            assert!(request.contains("If-None-Match: \"cached\"\r\n"));
            assert!(request.contains("If-Modified-Since: Sun, 06 Nov 1994 08:49:37 GMT\r\n"));
            stream
                .write_all(
                    b"HTTP/1.1 304 Not Modified\r\nETag: \"cached\"\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let previous = SubscriptionSource {
            entries: Default::default(),
            etag: Some("\"cached\"".to_owned()),
            last_modified: Some("Sun, 06 Nov 1994 08:49:37 GMT".to_owned()),
        };
        let response = fetch_subscription(
            "127.0.0.1",
            address.port(),
            "http://lists.i2p/hosts.txt",
            Some(&previous),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        proxy.await.unwrap();
        assert_eq!(response.status, 304);
        assert!(response.body.is_empty());
        assert_eq!(response.etag.as_deref(), Some("\"cached\""));
    }
}
