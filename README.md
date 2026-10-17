# opencay-proxy

HTTP proxy exposing the OpenAI-compatible OpenCode API for harnesses such as Pi.

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

## Logging & CLI

    cargo run                          # info logs, port 8310 (OPCAY_PORT/PORT env respected)
    cargo run -- -p 9000               # custom port
    cargo run -- -d                    # debug logs
    cargo run -- -v                    # trace logs
    cargo run -- --log-level warn      # explicit level
    RUST_LOG=debug cargo run           # RUST_LOG always wins over flags

Colored, leveled logs cover startup, requests (debug), retries (warn),
failures (error), and tool injection (debug).
