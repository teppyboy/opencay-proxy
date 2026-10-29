# opencay-proxy

HTTP proxy exposing the OpenAI-compatible OpenCode API for harnesses such as Pi.

## Requirements

The OpenCode free tier inspects the TLS ClientHello and rejects stacks whose
cipher list differs from the official CLI's (Bun). The proxy therefore shells
out to [curl-impersonate](https://github.com/lexiforest/curl-impersonate)
instead of using a plain TLS client.

```bash
# Linux x86_64
curl -fsSL -o /tmp/ci.tar.gz \
  https://github.com/lexiforest/curl-impersonate/releases/download/v2.3.1/curl-impersonate-v2.3.1.x86_64-linux-gnu.tar.gz
tar xzf /tmp/ci.tar.gz -C /tmp
sudo install -m755 /tmp/curl-impersonate /usr/local/bin/
```

Set `OPENCAY_CURL_BINARY` to point at a different binary.

## Run

```bash
cargo run
```

Listens on `http://127.0.0.1:8310`. Set `OPCAY_PORT` or `PORT` to override the port:

```bash
OPCAY_PORT=9000 cargo run
```

## Pi / harness configuration

Point the OpenAI-compatible provider base URL at the proxy:

```json
{
  "baseURL": "http://127.0.0.1:8310/v1",
  "apiKey": "unused",
  "model": "glm-5.3-flash-free"
}
```

The proxy always uses a streaming request upstream. It returns SSE when the client sends `"stream": true`, otherwise it aggregates the stream into one OpenAI chat completion.

Upstream errors are passed through unchanged; the proxy never retries and never
switches keys on failure.

## Logging & CLI

    cargo run                          # info logs, port 8310 (OPCAY_PORT/PORT env respected)
    cargo run -- -p 9000               # custom port
    cargo run -- --host 0.0.0.0        # expose on your LAN (default: localhost only)
    cargo run -- -d                    # debug logs
    cargo run -- -v                    # trace logs
    cargo run -- --log-level warn      # explicit level
    RUST_LOG=debug cargo run           # RUST_LOG always wins over flags

Colored, leveled logs cover startup, requests (debug), failures (error), and
tool injection (debug).
