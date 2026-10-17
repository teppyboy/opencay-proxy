# OpenCode client identity reimplementation

Source snapshot: `/tmp/opencode-research`, commit `4a5302817b5273d67117e67c787feb56df45e0de`.

## 1. Session ID generation

### Observable format

`SessionID.create()` returns the literal string `"ses_" + descending()`: the prefix is `ses_`, followed by the 26-character identifier returned by `Identifier.descending()` (`/tmp/opencode-research/packages/schema/src/session-id.ts:5-12`). Therefore a newly generated ID is 30 ASCII characters total: 4 prefix characters plus 26 identifier characters (`/tmp/opencode-research/packages/schema/src/session-id.ts:9`; `/tmp/opencode-research/packages/schema/src/identifier.ts:3,31`).

The normal session ID factory is descending, not ascending: `SessionID.create()` calls `descending()` (`/tmp/opencode-research/packages/schema/src/session-id.ts:8-12`), and `descending()` calls `create(true)` (`/tmp/opencode-research/packages/schema/src/identifier.ts:12-14`).

### Exact algorithm

Constants and state:

- Identifier length: `26` characters (`/tmp/opencode-research/packages/schema/src/identifier.ts:3`).
- Random suffix alphabet, in exact index order: `0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz` (`/tmp/opencode-research/packages/schema/src/identifier.ts:4`).
- Module-global `lastTimestamp = 0` and `counter = 0` (`/tmp/opencode-research/packages/schema/src/identifier.ts:5-6`).
- Default timestamp: JavaScript `Date.now()`, i.e. Unix epoch milliseconds (`/tmp/opencode-research/packages/schema/src/identifier.ts:16`).

Pseudocode equivalent for the session factory:

```text
ALPHABET = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
IDENTIFIER_LENGTH = 26
last_timestamp = 0          // module-global
counter = 0                 // module-global

function session_id(): string:
    return "ses_" + identifier_create_descending()

function identifier_create_descending(timestamp_ms = current_unix_time_milliseconds()): string:
    if timestamp_ms != last_timestamp:
        last_timestamp = timestamp_ms
        counter = 0

    counter = counter + 1

    // BigInt arithmetic; 0x1000 is exactly 4096.
    current = uint_or_bigint(timestamp_ms) * 0x1000 + counter
    value = bitwise_not_in_infinite_twos_complement(current)  // JS BigInt ~current

    // Keep the low 48 bits, emitted as 6 bytes / 12 lowercase hex characters.
    time_hex = ""
    for index in 0 .. 5:
        byte = (value >> (40 - 8 * index)) & 0xff
        time_hex += lowercase_hex(byte, width = 2)

    random_bytes = cryptographically_secure_random_bytes(14)
    random_suffix = ""
    for byte in random_bytes:
        random_suffix += ALPHABET[byte % 62]

    return time_hex + random_suffix
```

The pseudocode follows the source operations exactly: reset the counter whenever the timestamp changes, increment it, form `BigInt(timestamp) * 0x1000n + BigInt(counter)`, complement it for descending IDs, extract six bytes from shifts `40, 32, 24, 16, 8, 0`, format each as two-digit lowercase hexadecimal, then map 14 random bytes through `byte % 62` into the alphabet (`/tmp/opencode-research/packages/schema/src/identifier.ts:16-31`). The 14-byte count is `length - 12`: the first 12 output characters are the six two-digit hex bytes, leaving 14 random characters (`/tmp/opencode-research/packages/schema/src/identifier.ts:3,25-31`).

Important implementation details:

- The timestamp is **not base62 encoded**. It contributes to a 48-bit value rendered as 12 lowercase hex digits (`/tmp/opencode-research/packages/schema/src/identifier.ts:25-29`).
- The `0x1000` multiplier reserves 12 low bits for the per-timestamp counter (`/tmp/opencode-research/packages/schema/src/identifier.ts:23`). The source has no overflow check or counter clamp (`/tmp/opencode-research/packages/schema/src/identifier.ts:16-24`).
- Descending mode is JavaScript BigInt bitwise complement before low-byte extraction (`/tmp/opencode-research/packages/schema/src/identifier.ts:23-27`). A Rust implementation must preserve the low 48 bits of `!current`; do not use a fixed-width complement without masking the extracted bytes equivalently (`/tmp/opencode-research/packages/schema/src/identifier.ts:24-29`).
- Random selection is `crypto.getRandomValues(new Uint8Array(14))`, then `byte % 62`; it is not rejection sampling (`/tmp/opencode-research/packages/schema/src/identifier.ts:30-31`).

The separate `ascending()` factory calls `create(false)` and is not used by `SessionID.create()` (`/tmp/opencode-research/packages/schema/src/identifier.ts:8-14`; `/tmp/opencode-research/packages/schema/src/session-id.ts:8-12`).

## 2. User-Agent

### Core format

`App.useragent(app)` returns exactly:

```text
opencode/<channel>/<version>/<name>
```

It interpolates `app.channel`, `app.version`, and `app.name` without normalization or quoting (`/tmp/opencode-research/packages/core/src/app.ts:24-26`). The default app values are `name = "opencode"`, `version = "unknown"`, `channel = "unknown"` (`/tmp/opencode-research/packages/core/src/app.ts:16-21`).

### Released CLI value

The CLI passes this app identity to the TUI as:

- `name`: `process.env.OPENCODE_CLIENT ?? OPENCODE_ARTIFACT` (`/tmp/opencode-research/packages/cli/src/commands/handlers/default.ts:95-100`).
- `version`: `OPENCODE_VERSION` (`/tmp/opencode-research/packages/cli/src/commands/handlers/default.ts:97-100`).
- `channel`: `process.env.OPENCODE_TUI_CHANNEL ?? OPENCODE_CHANNEL` (`/tmp/opencode-research/packages/cli/src/commands/handlers/default.ts:97-100`).

A normal released CLI build defines `OPENCODE_ARTIFACT` as `cli` (`/tmp/opencode-research/packages/cli/script/build.ts:190-195`). Its compiled Bun runtime is also given the exact runtime user-agent argument `--user-agent=opencode/${Script.channel}/${Script.version}/cli` (`/tmp/opencode-research/packages/cli/script/build.ts:176-184`). Therefore the stable released CLI sends:

```text
opencode/latest/<released-version>/cli
```

The release build's stable channel is `latest`: the release script returns `latest` when `OPENCODE_BUMP` is set, or when a non-preview `OPENCODE_VERSION` is supplied (`/tmp/opencode-research/packages/script/src/index.ts:26-36`), and the build embeds that channel as `OPENCODE_CHANNEL` (`/tmp/opencode-research/packages/cli/script/build.ts:190-194`). Preview/dev builds substitute their actual channel and version, preserving the same format (`/tmp/opencode-research/packages/cli/script/build.ts:176-184`). `OPENCODE_CLIENT` or `OPENCODE_TUI_CHANNEL`, when explicitly set, override the CLI's app name or TUI channel (`/tmp/opencode-research/packages/cli/src/commands/handlers/default.ts:97-100`).

## 3. Headers on model inference requests

There are two identity-producing request call sites, then one common HTTP transport that turns the request options into the wire request.

### A. Session model requests

`packages/core/src/session/model-request.ts` constructs the request header map (`/tmp/opencode-research/packages/core/src/session/model-request.ts:278-293`). The exact entries are:

| Header | Exact value | Presence |
|---|---|---|
| `x-opencode-session-id` | `session.id` | Required for this session request; always inserted (`/tmp/opencode-research/packages/core/src/session/model-request.ts:282-284`). |
| `x-opencode-parent-session-id` | `session.parentID` | Optional; inserted only when `session.parentID` is truthy (`/tmp/opencode-research/packages/core/src/session/model-request.ts:283-285`). |
| `x-session-affinity` | `affinity`, where `affinity = SessionAffinity.get(session)` | Required for this session request; always inserted (`/tmp/opencode-research/packages/core/src/session/model-request.ts:278,285-286`). |
| `X-Session-Id` | `affinity` | Required for this session request; always inserted, with this exact capitalization in the request map (`/tmp/opencode-research/packages/core/src/session/model-request.ts:285-287`). |
| `x-parent-session-id` | `session.parentID` | Optional; inserted only when `session.parentID` is truthy (`/tmp/opencode-research/packages/core/src/session/model-request.ts:286-288`). |
| `User-Agent` | `App.useragent(app)`, exactly `opencode/<channel>/<version>/<name>` | Required for this session request; always inserted (`/tmp/opencode-research/packages/core/src/session/model-request.ts:287-289`; format `/tmp/opencode-research/packages/core/src/app.ts:24-26`). |
| `x-opencode-project` | `session.projectID` | Required for this session request; always inserted (`/tmp/opencode-research/packages/core/src/session/model-request.ts:288-290`). |
| `x-opencode-session` | `affinity` | Required for this session request; always inserted (`/tmp/opencode-research/packages/core/src/session/model-request.ts:289-291`). |
| `x-opencode-client` | `app.name` | Required for this session request; always inserted (`/tmp/opencode-research/packages/core/src/session/model-request.ts:290-292`). |

The two parent headers are independently conditional; omit both when `session.parentID` is absent (`/tmp/opencode-research/packages/core/src/session/model-request.ts:283-288`). `affinity` is reused verbatim for all three affinity headers (`x-session-affinity`, `X-Session-Id`, and `x-opencode-session`) (`/tmp/opencode-research/packages/core/src/session/model-request.ts:278,285-291`).

### B. Stateless `Generate.text` requests

`Generate.text` creates a fresh session ID and sends only one caller-supplied identity header in its `LLM.request` (`/tmp/opencode-research/packages/core/src/generate.ts:64-71`):

| Header | Exact value | Presence |
|---|---|---|
| `x-opencode-session` | `SessionID.create()`; therefore `ses_` plus the descending 26-character identifier described in §1 | Required for this call; always inserted (`/tmp/opencode-research/packages/core/src/generate.ts:69-71`). |

The source comment states that this attribution is required by gateways even though no `Session` is stored (`/tmp/opencode-research/packages/core/src/generate.ts:69-70`). This call does **not** add the session-model request's `User-Agent`, project, client, affinity, or parent headers at this call site (`/tmp/opencode-research/packages/core/src/generate.ts:66-71`).

### C. Common AI HTTP transport

`jsonRequestParts` builds the outgoing header collection by merging provider route headers first, then `request.http?.headers` over them (`/tmp/opencode-research/packages/ai/src/route/transport/http.ts:52-69`). Consequently, request-level headers from §§3A–3B override same-name route defaults (`/tmp/opencode-research/packages/ai/src/route/transport/http.ts:64-67`). In the normal LLM compiler path, the route's header function is evaluated and merged into `request.http` before transport preparation (`/tmp/opencode-research/packages/ai/src/route/client.ts:555-570`), so those route defaults participate in the same final merge. The headers are then passed through the configured auth definition (`/tmp/opencode-research/packages/ai/src/route/transport/http.ts:59-68`); auth may add or replace authentication headers. Auth implementations set values on the existing header collection (`/tmp/opencode-research/packages/ai/src/route/auth.ts:53-57,102-110`), and `Auth.none` leaves it unchanged (`/tmp/opencode-research/packages/ai/src/route/auth.ts:111-115`).

For ordinary JSON model inference over `http-json`, the common transport sends these additional/transport-level headers:

| Header | Exact value | Presence |
|---|---|---|
| `content-type` | `application/json` | Always required/forced by `jsonPost`; it overwrites any caller-supplied content type (`/tmp/opencode-research/packages/ai/src/protocols/shared.ts:401-408`). The HTTP transport calls `jsonPost` with the merged headers (`/tmp/opencode-research/packages/ai/src/route/transport/http.ts:82-93`). |
| `content-encoding` | `zstd` or `gzip`, matching `request.http?.encoding` when compression succeeds | Optional; present only when encoding is requested and runtime compression returns bytes (`/tmp/opencode-research/packages/ai/src/protocols/shared.ts:407-416,420-428`). If encoding is absent or `none`, or compression is unavailable/fails, it is omitted and plain JSON is sent (`/tmp/opencode-research/packages/ai/src/protocols/shared.ts:420-428`). |
| Authentication header(s) | Determined by the route's `Auth.Definition`; commonly `Authorization: Bearer <credential>` or a provider-specific header | Conditional; auth is applied after route/request header merge (`/tmp/opencode-research/packages/ai/src/route/transport/http.ts:59-68`). Bearer auth renders `authorization: Bearer <secret>` (`/tmp/opencode-research/packages/ai/src/route/auth.ts:48-57`). |

The complete wire-header rule for a JSON inference POST is therefore:

```text
wire_headers = request_headers_overriding(route_headers)
wire_headers = auth_apply(wire_headers, request, POST, url, encoded_json_body)
wire_headers["content-type"] = "application/json"
if compression_requested_and_successful:
    wire_headers["content-encoding"] = "zstd" or "gzip"
```

`http.ts` does not add an `Accept` header or any other model-identity header itself; all headers besides the JSON/compression headers above come from the route, the request, or auth (`/tmp/opencode-research/packages/ai/src/route/transport/http.ts:59-69,82-93`). The HTTP runtime may add protocol-level headers such as `Host` or `Content-Length`; those are outside the OpenCode source-defined identity set documented here.

The final JSON POST uses these headers and either the plain JSON text body or compressed bytes (`/tmp/opencode-research/packages/ai/src/protocols/shared.ts:407-417`). The transport's request-level identity headers are therefore preserved on the wire alongside `content-type`, optional `content-encoding`, and any auth headers (`/tmp/opencode-research/packages/ai/src/route/transport/http.ts:64-69,82-93`; `/tmp/opencode-research/packages/ai/src/protocols/shared.ts:407-417`). Provider route defaults are part of the wire set because the normal compiler supplies `route.headers` to transport preparation (`/tmp/opencode-research/packages/ai/src/route/client.ts:555-570`; `/tmp/opencode-research/packages/ai/src/route/client.ts:450-552`).

### Header-name casing

The source request maps use the exact spellings shown above, including `User-Agent` and `X-Session-Id` (`/tmp/opencode-research/packages/core/src/session/model-request.ts:282-292`). HTTP header names are case-insensitive on the wire; the implementation nevertheless preserves these source spellings until the HTTP client normalizes them (`/tmp/opencode-research/packages/ai/src/route/transport/http.ts:64-69`; `/tmp/opencode-research/packages/ai/src/protocols/shared.ts:407-417`).
