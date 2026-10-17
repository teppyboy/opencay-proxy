# OpenCode inference API and Console auth research

Research date: 2026-10-17 UTC. Sources are official OpenCode documentation fetched live from `opencode.ai`, the live models endpoint, and the checked-out source at `/tmp/opencode-research`.

## Current documentation paths

The older path `https://opencode.ai/docs/console/api/inference/` returns HTTP `404`:

```json
{"status":404,"message":"Cannot find any route matching [GET] https://opencode.ai/docs/console/api/inference/"}
```

Current versioned paths:

- Models and model endpoints: https://opencode.ai/v2/docs/console/models/
- Inference API: https://opencode.ai/v2/docs/console/api/inference/
- Console API, service accounts, user tokens, and device flow: https://opencode.ai/v2/docs/console/api/
- Console overview: https://opencode.ai/v2/docs/console/
- Connected providers: https://opencode.ai/v2/docs/console/providers/
- Workspaces: https://opencode.ai/v2/docs/console/workspaces/

The current pages use shortened links such as `/console/api/inference`; the canonical fetched URLs above include `/v2/docs`.

## Official inference documentation

### API families and base URL

Source: https://opencode.ai/v2/docs/console/api/inference/

Exact quote:

> Console inference serves OpenAI, Anthropic, and Gemini-compatible APIs from `https://opencode.ai/inference`. Send a JSON `POST` request to the endpoint that matches the model's API family.

The documented endpoint paths are:

| API family | Endpoint |
| --- | --- |
| OpenAI Chat Completions | `https://opencode.ai/inference/openai/v1/chat/completions` |
| OpenAI Responses | `https://opencode.ai/inference/openai/v1/responses` |
| Anthropic Messages | `https://opencode.ai/inference/anthropic/v1/messages` |
| Gemini | `https://opencode.ai/inference/google/v1beta/models/<model>:generateContent` |

Exact quote:

> Each model is served by one API family. See the [model endpoints](/console/models#endpoints) table, or fetch the live list:

The live-list command documented there is:

```bash
curl https://opencode.ai/inference/v1/models
```

### Authentication requirements

Source: https://opencode.ai/v2/docs/console/api/inference/

Exact quote:

> Replace `<token>` with a [service account key](/console/api#service-accounts) created in the [Console](https://opencode.ai/console). Paid models require the header; free chat models can be called without it.

The documented authenticated header is:

```text
Authorization: Bearer <token>
```

The same page states:

> Open models such as `kimi-k2.6`, `glm-5.1`, `minimax-m2.7`, and the free models use the Chat Completions API.

The docs' model-specific Jev example is different: it explicitly sends an API key:

```bash
curl https://opencode.ai/inference/systemone/v1/systemone \
  -H "Authorization: Bearer $OPENCODE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "jev-1.13",
    "state": "My payments have failed for three days and I am losing sales. Please help now.",
    "questions": {
      "is_urgent": {
        "type": "noul",
        "instructions": "Does this request require urgent attention?"
      }
    }
  }'
```

### Free-model statements

Source: https://opencode.ai/v2/docs/console/models/

Exact quotes from the `Free models` section:

> GLM-5.3-Flash Free is free on OpenCode for a limited time. Its provider follows a zero-retention policy and does not use your data for model training.

> DeepSeek V4.1 Flash Free is free on OpenCode for a limited time. Its provider follows a zero-retention policy and does not use your data for model training.

> Exo Free is available on OpenCode for a limited time. The team is using this time to collect feedback and improve the model.

> MiMo-V2.6-Flash Free is available on OpenCode for a limited time. The team is using this time to collect feedback and improve the model.

> MiMo-V2.5 Free is available on OpenCode for a limited time. The team is using this time to collect feedback and improve the model.

> Nemotron 3 Ultra Free is available on OpenCode for a limited time. The team is using this time to collect feedback and improve the model.

> Nemotron 3.5 Lightning Free is available on OpenCode for a limited time. The team is using this time to collect feedback and improve the model.

> Nemotron 3.5 Super VL Free is available on OpenCode for a limited time. The team is using this time to collect feedback and improve the model.

> Big Pickle is a stealth model that's free on OpenCode for a limited time. The team is using this time to collect feedback and improve the model.

> Space Bunny Free is a stealth model that's free on OpenCode for a limited time. Its provider follows a zero-retention policy and does not use your data for model training.

> Beam Free is free on OpenCode for a limited time. Its provider does not use your data for model training and retains it for 30 days for safety reasons. It's not available in Angola, Belarus, Cambodia, China (including Hong Kong and Macau), Cuba, the Democratic Republic of the Congo, Equatorial Guinea, Iran, Iraq, Laos, Libya, Nigeria, North Korea, Russia, Syria, Turkmenistan, Uzbekistan, Venezuela, or the Crimea, Donetsk, and Luhansk regions of Ukraine.

> LL Alpha Free is a stealth model that's free on OpenCode for a limited time. The team is using this time to collect feedback and improve the model.

> LongCat 2.5 Preview Free is free on OpenCode for a limited time. Its provider follows a zero-retention policy and does not use your data for model training.

> Jev 1.13 Free is available on OpenCode for a limited time.

The pricing table also marks the free models' input/output (and, where applicable, cached-read) price as `Free`.

## Live model-list check

Request:

```bash
curl -L https://opencode.ai/inference/v1/models
```

It succeeded without an `Authorization` header: HTTP `200`, `content-type: application/json`, `cache-control: no-store`. The response was an OpenAI-style list (`"object":"list"`) containing 85 models. The current IDs matching `*-free`, plus `big-pickle`, were:

```text
big-pickle
beam-free
mimo-v2.6-flash-free
nemotron-3.5-super-vl-free
nemotron-3.5-lightning-free
jev-1.13-free
longcat-2.5-preview-free
deepseek-v4.1-flash-free
glm-5.3-flash-free
space-bunny-free
```

This live list is authoritative for availability at check time. It does **not** include every free model named in the model documentation (for example, `exo-free`, `mimo-v2.5-free`, `nemotron-3-ultra-free`, and `ll-alpha-free` were documented but absent from this response).

## Models page endpoint table and Jev exception

Source: https://opencode.ai/v2/docs/console/models/

The endpoint table maps normal free chat models to:

```text
https://opencode.ai/inference/openai/v1/chat/completions
```

The table maps both Jev IDs to a different endpoint:

```text
jev-1.13       -> https://opencode.ai/inference/systemone/v1/systemone
jev-1.13-free  -> https://opencode.ai/inference/systemone/v1/systemone
```

Exact quote:

> Use your OpenCode Console API key with the `https://opencode.ai/inference/systemone/v1/systemone` endpoint.

Exact quote:

> Use `jev-1.13-free` instead of `jev-1.13` to use the limited-time free model. See the [TypeSafe AI documentation](https://docs.typesafe.ai/) for details about question types and response fields.

Therefore `jev-1.13-free` is free, but it is **not** an OpenAI Chat Completions model. A client must use the System One request shape (`state` plus typed `questions`) at `/inference/systemone/v1/systemone`, not `/inference/openai/v1/chat/completions`.

## Device-flow OAuth from official source

Source file:

- Local: `/tmp/opencode-research/packages/core/src/plugin/provider/opencode.ts`
- Checked-out revision: `4a5302817b5273d67117e67c787feb56df45e0de`
- Repository: https://github.com/anomalyco/opencode/blob/dev/packages/core/src/plugin/provider/opencode.ts

### Constants and endpoint base

The source sets:

```ts
const defaultServer = "https://opencode.ai/console"
const clientID = "opencode-cli"
```

Thus the default OAuth endpoints are:

```text
POST https://opencode.ai/console/auth/device/code
POST https://opencode.ai/console/auth/device/token
```

The source accepts a custom HTTP(S) server value, normalizes it by removing trailing slashes, and appends the same paths.

### 1. Device-code request

The source calls `/auth/device/code` with a JSON body:

```json
{
  "client_id": "opencode-cli",
  "supports_org_scope": true
}
```

Relevant source statement:

```ts
const device = yield* post(
  http,
  `${server}/auth/device/code`,
  { client_id: clientID, supports_org_scope: true },
  Device,
)
```

`post()` uses `HttpClientRequest.schemaBodyJson(...)`, so this reimplementation should send JSON, not assume a form-encoded body.

The response schema requires these fields and types:

```ts
const Device = Schema.Struct({
  device_code: Schema.String,
  user_code: Schema.String,
  verification_uri_complete: Schema.String,
  expires_in: Schema.Number,
  interval: Schema.Number,
})
```

The client opens `verification_uri_complete` after validating that its URL scheme is `http:` or `https:`. It displays:

```text
Enter code: <user_code>
```

### 2. Polling for the token

The source waits `interval` seconds, then posts JSON to:

```text
POST https://opencode.ai/console/auth/device/token
```

Body:

```json
{
  "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
  "device_code": "<device_code>",
  "client_id": "opencode-cli"
}
```

The source accepts either a successful token or `{ "error": "<string>" }` while polling:

```ts
const Token = Schema.Struct({
  access_token: Schema.String,
  refresh_token: Schema.String,
  expires_in: Schema.Number,
  org_id: Schema.optional(Schema.NullOr(Schema.String)),
})
const TokenPending = Schema.Struct({ error: Schema.String })
const DeviceToken = Schema.Union([Token, TokenPending])
```

Polling behavior is exact:

- `access_token` present: finish login, then fetch user/org metadata.
- `error === "authorization_pending"`: wait the same interval and poll again.
- `error === "slow_down"`: add 5 seconds to the wait duration for the next poll.
- Any other error: fail with `Device authorization failed: <error>`.

After token receipt, the source calls these endpoints concurrently, with `Authorization: Bearer <access_token>`:

```text
GET https://opencode.ai/console/api/user
GET https://opencode.ai/console/api/orgs
```

Response schemas:

```ts
const User = Schema.Struct({ id: Schema.String, email: Schema.String })
const Org = Schema.Struct({ id: Schema.String, name: Schema.String })
```

The selected organization is `org_id` when present; otherwise the first organization sorted by name, then ID. The resulting local credential stores `server`, `accountID`, `email`, `orgID`, and `orgName` metadata, plus the access/refresh tokens and an expiry timestamp of `Date.now() + expires_in * 1000`.

### 3. Refresh flow

Refresh uses the same token endpoint and JSON body:

```json
{
  "grant_type": "refresh_token",
  "refresh_token": "<stored_refresh_token>",
  "client_id": "opencode-cli"
}
```

Relevant source call:

```ts
const response = yield* send(http, `${serverUrl(credential)}/auth/device/token`, {
  grant_type: "refresh_token",
  refresh_token: credential.refresh,
  client_id: clientID,
})
```

A successful refresh requires the same `Token` fields (`access_token`, `refresh_token`, `expires_in`, optional nullable `org_id`). The source replaces both stored tokens and recomputes expiry. If a HTTP 400 body decodes as `{ "error": "invalid_grant" }`, it treats the session as signed out and requires a new device login.

The official API docs additionally state at https://opencode.ai/v2/docs/console/api/:

> Each refresh token can be used once. Reusing an old refresh token revokes the whole session.

### Authenticated-request headers

The source's shared `credentialHeaders()` is exact:

```ts
function credentialHeaders(value: Credential.Key | Credential.OAuth): Record<string, string> {
  const orgID = value.metadata?.orgID
  return {
    authorization: `Bearer ${value.type === "oauth" ? value.access : value.key}`,
    ...(typeof orgID === "string" ? { "x-org-id": orgID } : {}),
  }
}
```

For an OAuth access token, authenticated Console requests therefore use:

```text
Authorization: Bearer <access_token>
x-org-id: <orgID>       # include when credential metadata has an orgID
```

For a service/API key, the bearer value is the key instead. `x-org-id` is added only when an organization ID exists in metadata.

The source's `GET /api/user` and `GET /api/orgs` helper sends only `Authorization: Bearer <access_token>`; organization selection happens after those calls. The source's Console web-search request sends both bearer authorization and `x-org-id` when available. The provider config request (`GET /api/v2/config`) uses the same `credentialHeaders()` pair.

The current official API docs qualify the organization-header behavior. Exact quote from https://opencode.ai/v2/docs/console/api/:

> User tokens act as a signed-in person with their workspace role. Send the token with an `x-org-id` header naming the workspace.

Exact quote:

> Tokens from the [device flow](#device-flow) are bound to the workspace chosen during sign-in and do not need the header. A different `x-org-id` returns `403`.

For a reimplementation matching the source, preserve the header rule above: always send `Authorization: Bearer ...` on authenticated inference/Console requests; send `x-org-id` when an org is selected/stored, while recognizing that a device-flow token is already workspace-bound.

## Official source/docs caveats

- The docs say free **chat** models may be called without `Authorization`; the Jev example documents an API key and uses a distinct System One endpoint.
- The live unauthenticated check here was only `GET /inference/v1/models`; it confirms model-list access, not successful unauthenticated inference.
- Free-model availability is time-dependent. Use the live models endpoint before hard-coding IDs.
