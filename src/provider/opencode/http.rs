//! HTTP transport for opencode.ai.
//!
//! The free tier inspects the TLS ClientHello. The official CLI runs on Bun,
//! whose JA4 cipher-list hash is `5b57614c22b0`; a stock TLS stack presents a
//! different list and is refused. curl-impersonate links BoringSSL and can
//! emit Bun's exact cipher list, so we shell out to it.
//!
//! Every opencode.ai request must use this transport: one IP presenting two
//! different fingerprints is itself a bot signal.

use bytes::Bytes;
use futures_util::Stream;
use std::io::Error as IoError;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdout};
use tokio_util::io::ReaderStream;

use super::BoxError;

pub type HeaderList = Vec<(String, String)>;

/// curl-impersonate binary to spawn. Override with `OPENCAY_CURL_BINARY`.
pub fn binary() -> String {
    std::env::var("OPENCAY_CURL_BINARY").unwrap_or_else(|_| "curl-impersonate".to_string())
}

/// Bun's TLS cipher list, verbatim.
///
/// The upstream compares the JA4 cipher-list hash against Bun's
/// (`5b57614c22b0`) and nothing else, so this list must stay exactly as it is.
/// It is Chrome's list plus the two SHA-256 suites Bun still offers.
const BUN_CIPHER_LIST: &str = "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:ECDHE-ECDSA-AES128-SHA:ECDHE-RSA-AES128-SHA:ECDHE-ECDSA-AES256-SHA:ECDHE-RSA-AES256-SHA:AES128-GCM-SHA256:AES256-GCM-SHA384:AES128-SHA:AES256-SHA";

/// Impersonation target. `:no` drops the browser's navigation headers, which
/// do not belong on an API request; the TLS and HTTP/2 fingerprints stay.
const TARGET: &str = "chrome136:no";

/// POST and return the response body as a byte stream.
///
/// Returns the HTTP status plus a stream of body bytes. The body is not
/// buffered, so SSE responses stream through as they arrive.
pub async fn post_stream(
    url: &str,
    headers: &HeaderList,
    body: &str,
) -> Result<(u16, impl Stream<Item = Result<Bytes, IoError>> + Send), BoxError> {
    let (status, reader) = spawn("POST", url, headers, Some(body)).await?;
    Ok((status, to_stream(reader)))
}

/// GET and return the response body as text.
pub async fn get_text(url: &str, headers: &HeaderList) -> Result<(u16, String), BoxError> {
    let (status, mut reader) = spawn("GET", url, headers, None).await?;
    let mut body = String::new();
    reader.read_to_string(&mut body).await?;
    Ok((status, body))
}

async fn spawn(
    method: &str,
    url: &str,
    headers: &HeaderList,
    body: Option<&str>,
) -> Result<(u16, CurlStream), BoxError> {
    let binary = binary();
    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg("-sS") // show errors, hide the progress meter
        .arg("-N") // do not buffer: stream the response as it arrives
        .arg("-i") // prepend the response headers to the body
        .arg("--compressed")
        .arg("--impersonate")
        .arg(TARGET)
        .arg("--ciphers")
        .arg(BUN_CIPHER_LIST)
        .arg("-X")
        .arg(method);
    for (name, value) in headers {
        cmd.arg("-H").arg(format!("{name}: {value}"));
    }
    if body.is_some() {
        // Read the body from stdin so large conversations do not hit ARG_MAX.
        cmd.arg("--data-binary").arg("@-");
    }
    cmd.arg(url)
        .stdin(if body.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|error| {
        format!(
            "failed to spawn curl-impersonate binary '{binary}': {error}. \
             Install curl-impersonate or point OPENCAY_CURL_BINARY at it."
        )
    })?;
    if let (Some(payload), Some(mut stdin)) = (body, child.stdin.take()) {
        stdin.write_all(payload.as_bytes()).await?;
        stdin.shutdown().await?;
    }
    let stdout = child.stdout.take().expect("stdout is piped");
    let (status, reader) = split_http_response(stdout).await?;
    Ok((status, CurlStream { reader, _child: child }))
}

/// Split the leading HTTP status line and headers off the body stream.
async fn split_http_response(mut stdout: ChildStdout) -> Result<(u16, CombinedReader), BoxError> {
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    loop {
        let read = stdout.read(&mut chunk).await?;
        if read == 0 {
            return Err("curl-impersonate produced no response headers".into());
        }
        buf.extend_from_slice(&chunk[..read]);
        if buf.windows(4).any(|window| window == b"\r\n\r\n")
            || buf.windows(2).any(|window| window == b"\n\n")
        {
            break;
        }
        if buf.len() > 64 * 1024 {
            return Err("response headers exceeded 64KB".into());
        }
    }
    let status = String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let body_start = find_body_start(&buf);
    let leftover = (body_start < buf.len()).then(|| buf[body_start..].to_vec());
    Ok((status, CombinedReader { leftover, inner: stdout }))
}

/// Offset of the first body byte, i.e. just past the header terminator.
fn find_body_start(buf: &[u8]) -> usize {
    if let Some(position) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
        return position + 4;
    }
    if let Some(position) = buf.windows(2).position(|window| window == b"\n\n") {
        return position + 2;
    }
    buf.len()
}

/// Yields bytes read past the header terminator before continuing with stdout.
struct CombinedReader {
    leftover: Option<Vec<u8>>,
    inner: ChildStdout,
}

impl AsyncRead for CombinedReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if let Some(leftover) = self.leftover.as_mut() {
            if !leftover.is_empty() {
                let take = std::cmp::min(buffer.remaining(), leftover.len());
                buffer.put_slice(&leftover[..take]);
                leftover.drain(..take);
                if leftover.is_empty() {
                    self.leftover = None;
                }
                return std::task::Poll::Ready(Ok(()));
            }
            self.leftover = None;
        }
        std::pin::Pin::new(&mut self.inner).poll_read(context, buffer)
    }
}

/// The live curl child plus its stdout; killing the child on drop aborts the
/// upstream request when the downstream client disconnects.
struct CurlStream {
    reader: CombinedReader,
    _child: Child,
}

impl AsyncRead for CurlStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.reader).poll_read(context, buffer)
    }
}

fn to_stream(reader: CurlStream) -> impl Stream<Item = Result<Bytes, IoError>> + Send {
    ReaderStream::new(reader)
}

#[cfg(test)]
mod tests {
    use super::find_body_start;

    #[test]
    fn finds_the_body_after_crlf_headers() {
        let buf = b"HTTP/2 200\r\ncontent-type: text/event-stream\r\n\r\ndata: hi";
        assert_eq!(find_body_start(buf), 47);
    }

    #[test]
    fn finds_the_body_after_bare_lf_headers() {
        let buf = b"HTTP/1.1 429\n\ndata: err";
        assert_eq!(find_body_start(buf), 14);
    }
}
