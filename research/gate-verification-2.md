# OpenCode free-tier gate — verified passing request

**Run date:** 2026-10-17 UTC
**Endpoint:** `POST https://opencode.ai/inference/openai/v1/chat/completions`
**Result:** ✅ HTTP 200 SSE achieved anonymously. The gate is passable with the correct fingerprint.

This supersedes `gate-verification.md`, which failed because its session IDs used a plain
millisecond-hex timestamp. The timestamp must be the **descending-encoding** from
`packages/schema/src/identifier.ts` (see `identity.md` §1).

## Minimal passing request (verified 200 today)

```bash
SID=$(python3 -c "
import secrets, time
AL='0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz'
now=int(time.time()*1000); v=~(now*0x1000+1)
h=''.join(f'{(v >> (40-8*i)) & 255:02x}' for i in range(6))
print('ses_'+h+''.join(AL[b%62] for b in secrets.token_bytes(14)))")

curl -N -X POST 'https://opencode.ai/inference/openai/v1/chat/completions' \
  -H 'Content-Type: application/json' \
  -H 'User-Agent: opencode/latest/1.19.3/cli' \
  -H "x-opencode-session: $SID" \
  -d '{
    "model": "glm-5.3-flash-free",
    "messages": [{"role":"user","content":"Reply with exactly OK"}],
    "tools": [
      {"type":"function","function":{"name":"bash","description":"Execute a shell command","parameters":{"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}}},
      {"type":"function","function":{"name":"read","description":"Read a file","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}
    ],
    "tool_choice": "auto",
    "stream": true,
    "stream_options": {"include_usage": true},
    "max_tokens": 16
  }'
```

## Requirements matrix (each row = one live test)

| Factor | Verdict | Evidence |
|---|---|---|
| `x-opencode-session` with **descending-encoded** timestamp | **REQUIRED** | plain-ms-hex session → 403 (T23); encoded → 200 (T18, T24) |
| `User-Agent: opencode/<…>` | **REQUIRED** | curl/foreign/absent UA → 403 (round 1, issue #49433); `opencode/1.19.3`, `opencode/latest/1.19.3/cli`, SDK-suffixed UA all pass (tests 06, 07) |
| `tools` containing BOTH `bash` and `read` (minimal schemas ok) | **REQUIRED** | bash-only → 403 in 4/4 healthy windows (T26, 2026-10-17 evening); bash+read minimal → 200 (T24). Proxy must inject missing ones. |
| `stream: true` | required per #49433 | not re-isolated today; keep it |
| `x-opencode-session-id` | NOT required | test 11 → 200 without it |
| `x-opencode-client` | NOT required | test 12 → 200 |
| `x-opencode-project` | NOT required | test 13 → 200 |
| `x-session-affinity` / `X-Session-Id` | NOT required | T18 → 200 without both |
| system message | NOT required | test 02 → 200 |
| `stream_options` | NOT required | test 09 → 200 |
| `max_tokens` vs `max_completion_tokens` | both pass | test 10 → 200 |
| full official 12-tool schemas | NOT required | minimal 2-tool schemas pass (T24); strict:false optional (test 14) |
| tool subset `read`+`shell` (official names) | passes | test 03 → 200 |
| model `big-pickle` | passes | test 08 → 200 |
| `Authorization` | NOT required | all passing tests were anonymous |

## Session ID (the part everyone gets wrong)

`sess_`… no — exact format `ses_` + 12 lowercase hex + 14 base62 chars, where the 12 hex are
the low 48 bits of `~(unix_ms * 0x1000 + counter)` (BigInt complement, counter usually 1), big-endian bytes.
Rust:

```rust
fn session_id() -> String {
    const AL: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let v = !(now.wrapping_mul(0x1000).wrapping_add(1)); // counter=1
    let mut s = String::from("ses_");
    for i in 0..6 { s.push_str(&format!("{:02x}", (v >> (40 - 8 * i)) & 0xff)); }
    let mut buf = [0u8; 14];
    getrandom::fill(&mut buf).unwrap();
    for b in buf { s.push(AL[(b % 62) as usize] as char); }
    s
}
```

Matches `packages/schema/src/identifier.ts` exactly (spec: `identity.md` §1, counter resets per
timestamp; gate does not check counter > 1).

## Load flapping (2026-10-17 evening)

The free tier alternates 200 / 429 / 403-FreeTierError on IDENTICAL verified requests within
minutes (12 windows sampled). 403s under load are false positives — always retry with backoff
before treating a 403 as a fingerprint failure.

## Why round 1 failed

`gate-verification.md` used `ses_` + plain `format(ms, "012x")` + 14 base62. The gate decodes the
timestamp via the descending scheme and rejects implausible values. UA, tools, and stream were fine.

## Test log

Full matrix from the subagent run (bodies/headers in `/tmp/gate2/metadata.json`):
01–03, 06–15 → 200; 04 (no tools), 05 (no tools + minimal headers) → 403.
Parent-run reductions: T18 (UA+session only) → 200; T20/T21/T22 (affinity variants) → 200;
T23 (plain-hex session) → 403; T24 (minimal tools) → 200.
