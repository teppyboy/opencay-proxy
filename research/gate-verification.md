# OpenCode free-tier gate verification

**Run date:** 2026-10-17 UTC  
**Endpoint:** `https://opencode.ai/inference/openai/v1/chat/completions`  
**Network:** direct `curl` from the repository host; no OpenCode client involved  
**Model:** `glm-5.3-flash-free`

## Result

The requested four-factor fingerprint did **not** produce a 200 today. Every anonymous request tested returned HTTP 403 with the same `FreeTierError`, including the full candidate request containing:

- `User-Agent: opencode/1.19.3`
- a freshly generated `ses_` session ID with a current millisecond timestamp encoded as 12 lowercase hex characters plus 14 base62 characters
- `tools` containing `bash` and `read`
- `stream: true`

Therefore there is no empirically verified minimal passing request to document for anonymous direct `curl` today. The four reported checks are insufficient at the current edge, or the edge applies an additional identity/request check.

Common response body, exact bytes observed in all 12 requests:

```json
{"type":"error","error":{"type":"FreeTierError","message":"OpenCode's free tier can only be used from within OpenCode. Download it at https://opencode.ai and keep building."}}
```

## Candidate full request

This was the first controlled request. The session was generated immediately before the request:

- millisecond timestamp: `1792237832160`
- 12-hex timestamp: `01a149b23d26`
- session: `ses_01a149b23d26020ILPTGL7ikbL`

```bash
curl -sS -N --max-time 30 -X POST \
  'https://opencode.ai/inference/openai/v1/chat/completions' \
  -H 'Content-Type: application/json' \
  -H 'Accept: text/event-stream' \
  -H 'User-Agent: opencode/1.19.3' \
  -H 'x-opencode-session: ses_01a149b23d26020ILPTGL7ikbL' \
  -d '{
    "model": "glm-5.3-flash-free",
    "messages": [{"role": "user", "content": "Reply with exactly OK"}],
    "stream": true,
    "tools": [
      {
        "type": "function",
        "function": {
          "name": "bash",
          "description": "Execute a shell command",
          "parameters": {
            "type": "object",
            "properties": {"command": {"type": "string"}},
            "required": ["command"]
          }
        }
      },
      {
        "type": "function",
        "function": {
          "name": "read",
          "description": "Read a file",
          "parameters": {
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"]
          }
        }
      }
    ],
    "tool_choice": "auto",
    "max_tokens": 16
  }' \
  -w '\nHTTP_STATUS:%{http_code}\n'
```

Observed result:

```text
HTTP_STATUS:403
{"type":"error","error":{"type":"FreeTierError","message":"OpenCode's free tier can only be used from within OpenCode. Download it at https://opencode.ai and keep building."}}
```

This is the closest available answer to an exact minimal request: it is the tested all-four candidate, but it is rejected rather than passing.

## One-factor controls

Each control used a newly generated current session except the deliberately malformed-session case. The request body and headers not named in the variation remained the same as the candidate.

| Test | Single variation | Status | Body |
|---|---|---:|---|
| Candidate | all four reported factors present | 403 | common `FreeTierError` above |
| No tools | removed `tools` entirely; `stream: true` retained | 403 | common `FreeTierError` above |
| Malformed session | `x-opencode-session: ses_AAAAAAAAAAAAabcdefghijkl` | 403 | common `FreeTierError` above |
| Non-streaming | changed only `stream` to `false` | 403 | common `FreeTierError` above |
| New OpenCode version | candidate shape with `User-Agent: opencode/2.0.32` | 403 | common `FreeTierError` above |

The negative controls cannot be interpreted as independent confirmation of each factor because the baseline itself fails. They only confirm that changing or removing each reported factor does not recover access from this network/client context.

## Additional checks

### `Authorization: Bearer public` does not recover access

Repeated the candidate and all three one-factor controls with:

```text
Authorization: Bearer public
```

All four returned HTTP 403 with the same body. This is not evidence that a real authenticated account would fail; no account credential was available or used.

### Current edge metadata

One candidate request captured these response headers:

```text
HTTP/2 403
date: Sat, 17 Oct 2026 11:40:43 GMT
content-type: application/json
content-length: 175
cf-placement: remote-IAD
x-opencode-log-id: 935c4d33-7496-4868-92d3-53c06b9098f5
server: cloudflare
cf-ray: a4bf07b058a67618-SEA
```

The service returned no `WWW-Authenticate`, rate-limit, or retry header. The response is an explicit application-level free-tier gate, not a transport failure.

### Endpoint check

`GET https://opencode.ai/inference/openai/v1/models` returned HTTP 404, so model discovery was not available through that path. This did not affect the POST experiments, which consistently reached the gate.

## Reproducibility

The controlled run used six POSTs without authorization, then four POSTs with `Authorization: Bearer public`, plus two preliminary candidate requests. Total: **12 POST requests**. Every response was HTTP 403 with the exact common body above; no SSE stream or `data: [DONE]` was observed.

The current evidence supports this narrower conclusion:

> As of 2026-10-17, an anonymous direct request to this endpoint is not accepted merely by spoofing the reported User-Agent, session shape, builtin tools, and streaming flag. A further server-side check, changed gate implementation, or account/client identity is required.

Community findings referenced for comparison: [anomalyco/opencode#49433](https://github.com/anomalyco/opencode/issues/49433), [#51315](https://github.com/anomalyco/opencode/issues/51315), and [#52907](https://github.com/anomalyco/opencode/issues/52907). Those reports establish the four-factor behavior in earlier/current official-client scenarios, but do not provide a passing anonymous direct-curl request from this run.
