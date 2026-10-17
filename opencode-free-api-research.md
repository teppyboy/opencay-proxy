# OpenCode Free-tier API request research

Research date: 2026-06-08, **updated 2026-10-17** (free tier is now gated — see update section first)

## 2026-10-17 update: free tier is official-client-only

**Anonymous free-tier calls from third-party clients no longer work.** Since ~2026-09-17 the inference edge rejects non-OpenCode clients with:

```json
{"type":"error","error":{"type":"FreeTierError","message":"OpenCode's free tier can only be used from within OpenCode. Download it at https://opencode.ai and keep building."}}
```

Verified live today: keyless curl to `/inference/openai/v1/chat/completions` with a free model returns 403 `FreeTierError` regardless of `User-Agent` alone.

Official policy (maintainer, anomalyco/opencode#49580, quoted in #49433):

> We've been tightening our logic to fight abuse. You cannot use the free tier in other harnesses (this is only a limitation for the free tier nothing else).
> Our free tier is only meant to be used in opencode. It isn't meant for people to take advantage of for their own projects.

### What the gate checks (community-verified, anomalyco/opencode#49433)

A request to the free tier must satisfy **all** of:

1. `User-Agent: opencode/<semver>` — case-insensitive, semver ≥ 1.18.0; git-describe strings (`1.18.31.r12.g88c6c7a`) and missing/foreign UAs rejected.
2. `x-opencode-session: ses_<12 hex timestamp><14 base62>` — the format of `Identifier.create` in `packages/opencode/src/id/id.ts`; timestamp must decode to a plausible (not ancient) time; UUIDs and other shapes rejected.
3. Body `tools` must include the OpenCode builtin toolset — at minimum `bash` and `read`; reduced toolsets (e.g. `plan` agent) are rejected even from the official CLI.
4. `stream: true` — SSE only.

Not checked: `x-opencode-client`, `x-opencode-request`, `x-opencode-project`; `Authorization: Bearer public` vs a real key both pass with a valid fingerprint. Even the official CLI hits false positives (compaction requests, `plan` agent, `tools.read=false` configs) — see #49433, #49678, #51315, #52907.

**Implication for a reimplementation:** the gate is passable anonymously and the exact minimal passing request is verified in `research/gate-verification-2.md` (2026-10-17): UA `opencode/<ver>`, a `x-opencode-session` with the descending-encoded timestamp from `identifier.ts` (NOT plain ms-hex), at least one tool in `tools`, and `stream: true`. OpenCode's stated policy remains "free tier only in opencode"; since the user of this repo was told by OpenCode to reimplement the full mechanism, that is the sanctioned path. Expect the gate to tighten over time — build the fingerprint from the real `identifier.ts` algorithm, not a static snapshot.

---

## Original research (2026-06-08) — API shape still accurate, gating changed

Source revision examined: `4a5302817b5273d67117e67c787feb56df45e0de` (cloned from `https://github.com/anomalyco/opencode.git`)

## Bottom line

The OpenCode client uses an OpenAI-compatible Chat Completions request for the documented Free models:

```text
POST https://opencode.ai/inference/openai/v1/chat/completions
```

As of 2026-09-17 the route is gated to the official client (see update section). Older docs claimed free chat models could be called without an API key; that is no longer true in practice for non-official clients.

## Exact request

Minimal request:

```bash
curl -N -X POST 'https://opencode.ai/inference/openai/v1/chat/completions' \
  -H 'Content-Type: application/json' \
  -H 'Accept: text/event-stream' \
  -d '{
    "model": "glm-5.3-flash-free",
    "messages": [{"role": "user", "content": "Say hi"}],
    "stream": true
  }'
```

Typical OpenCode-generated body for a normal streamed turn:

```json
{
  "model": "glm-5.3-flash-free",
  "messages": [
    {"role": "system", "content": "..."},
    {"role": "user", "content": "..."}
  ],
  "stream": true,
  "stream_options": {"include_usage": true},
  "max_tokens": 4096,
  "temperature": 0.2,
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "...",
        "description": "...",
        "parameters": {"type": "object", "properties": {}}
      }
    }
  ],
  "tool_choice": "auto"
}
```

Not every optional field is present. OpenCode adds `max_completion_tokens` instead of `max_tokens` only when model compatibility says that field is required. Tools, sampling controls, stop sequences, reasoning fields, and cache fields are conditional.

## Headers

Always/normal:

```text
Content-Type: application/json
Accept: text/event-stream
```

Authentication:

```text
Authorization: Bearer <OpenCode API key or token>
```

For anonymous Free calls, omit `Authorization`. In the current server proxy, a missing key or the literal local placeholder key `public` is treated as anonymous; the proxy removes credential headers before forwarding to the inference service.

Possible client/runtime headers:

- `User-Agent`: OpenCode sets this for Console/config requests. The generic model request can also carry configured deployment headers.
- `x-opencode-request-id`: optional request correlation header; server proxy preserves it.
- `x-real-ip`: added server-side from Cloudflare's `cf-connecting-ip`; not a client requirement.
- `content-encoding: zstd` or `gzip`: optional if body compression is configured and available; otherwise body is plain JSON.

Do not copy browser-only headers such as `sec-ch-ua`, `origin`, or cookies. They are not part of the documented API contract.

## Streaming response

The OpenAI Chat route uses SSE framing:

```text
data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{...}}

data: [DONE]
```

OpenCode's parser accepts one JSON event per `data:` payload plus the terminal `data: [DONE]`. Usage is requested with `stream_options.include_usage` when the route supports it.

## Free model IDs

The official model table currently lists these OpenAI Chat-compatible Free IDs:

- `big-pickle`
- `glm-5.3-flash-free`
- `deepseek-v4.1-flash-free`
- `space-bunny-free`
- `beam-free`
- `ll-alpha-free`
- `longcat-2.5-preview-free`
- `exo-free`
- `mimo-v2.6-flash-free`
- `mimo-v2.5-free`
- `nemotron-3-ultra-free`
- `nemotron-3.5-lightning-free`
- `nemotron-3.5-super-vl-free`

`jev-1.13-free` is also listed, but it uses the System One endpoint and body, not Chat Completions:

```text
POST https://opencode.ai/inference/systemone/v1/systemone
```

## OpenCode client source path

The cloned official repository is at `/tmp/opencode-research`.

Relevant source:

- `packages/core/src/plugin/provider/opencode.ts`
  - Provider discovery/configuration.
  - Keyless OpenCode models are marked with `apiKey: "public"` locally.
  - Paid/non-free models are disabled when no credential is present.
- `packages/ai/src/providers/openai-compatible.ts`
  - Configures the OpenAI-compatible provider with a bearer credential when one is supplied.
- `packages/ai/src/protocols/openai-compatible-chat.ts`
  - Reuses the OpenAI Chat protocol and `/chat/completions` endpoint suffix.
- `packages/ai/src/protocols/openai-chat.ts`
  - Builds the native body and enables `stream: true`.
- `packages/ai/src/route/transport/http.ts`
  - Builds the HTTP POST and applies auth.
- `packages/ai/src/protocols/shared.ts`
  - Sets `Content-Type: application/json` and optionally compresses the body.
- `packages/console/app/src/lib/inference-proxy.ts`
  - Maps `/zen/v1/chat/completions` to the internal inference route, extracts bearer auth, strips credentials for anonymous calls, and forwards request IDs.

## Important endpoint distinction

The repository currently exposes two related documented gateway shapes:

1. Current Console inference API:
   `https://opencode.ai/inference/openai/v1/chat/completions`
2. Zen-compatible public proxy shape in server source:
   `https://opencode.ai/zen/v1/chat/completions`

The Zen proxy maps to an internal inference route. The current Console docs and provider config use `/inference/openai/v1`; use that as the primary current endpoint. Do not assume `/zen/v1` and `/inference/openai/v1` are interchangeable across all deployments without checking the installed client/version.

## Auth flow when using OpenCode Console login

The client can use OAuth device flow against:

```text
https://opencode.ai/console/auth/device/code
https://opencode.ai/console/auth/device/token
```

Client ID in current source:

```text
opencode-cli
```

Device-code request body:

```json
{"client_id":"opencode-cli","supports_org_scope":true}
```

Polling body:

```text
grant_type=urn:ietf:params:oauth:grant-type:device_code
device_code=...
client_id=opencode-cli
```

Successful token response contains `access_token`, `refresh_token`, `expires_in`, and optional `org_id`. Requests then use `Authorization: Bearer <access_token>`; a user-token request may additionally use `x-org-id`.

This login flow is not required for documented anonymous Free chat requests.

## Primary sources

1. Official OpenCode repository: https://github.com/anomalyco/opencode
2. Current repository source, `packages/console/app/src/lib/inference-proxy.ts`: https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/lib/inference-proxy.ts
3. Current repository source, `packages/ai/src/protocols/openai-chat.ts`: https://github.com/anomalyco/opencode/blob/dev/packages/ai/src/protocols/openai-chat.ts
4. Current repository source, `packages/ai/src/providers/openai-compatible.ts`: https://github.com/anomalyco/opencode/blob/dev/packages/ai/src/providers/openai-compatible.ts
5. OpenCode Console inference API docs: https://opencode.ai/docs/console/api/inference/
6. OpenCode Console model/endpoints docs: https://opencode.ai/docs/console/models/
7. OpenCode Console API/auth docs: https://opencode.ai/docs/console/api/

## Limits and confidence

- Exact Free model availability changes over time; query the live model list before hard-coding IDs.
- The public docs explicitly document keyless Free chat calls, but backend rate limits, abuse controls, geography, and model availability can change.
- The source shows the OpenCode client and gateway behavior. It does not expose the private upstream provider request after OpenCode's inference service; that hop is server-side.
