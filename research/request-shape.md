# OpenCode OpenAI Chat request shape

Source: `/tmp/opencode-research`, commit `4a53028`.

## Scope and endpoint

The OpenAI-compatible route uses the provider's configured `baseURL` plus `/chat/completions` (`packages/ai/src/protocols/openai-compatible-chat.ts:10-20`). Therefore the requested deployment sends:

```text
POST https://opencode.ai/inference/openai/v1/chat/completions
```

when its configured base URL is `https://opencode.ai/inference/openai/v1`. `Endpoint.path` concatenates the trimmed base URL and route path (`packages/ai/src/route/endpoint.ts:29-36`). The native OpenAI route uses the same path (`packages/ai/src/protocols/openai-chat.ts:25-27,1312-1318`).

The JSON transport always sets `content-type: application/json` (`packages/ai/src/protocols/shared.ts:396-415`). Session orchestration supplies these request headers before provider authentication (`packages/core/src/session/model-request.ts:280-302`):

```text
x-opencode-session-id: <session id>
x-opencode-parent-session-id: <parent id>       # only for child sessions
x-session-affinity: <affinity>
X-Session-Id: <affinity>
x-parent-session-id: <parent id>                # only for child sessions
User-Agent: <OpenCode user agent>
x-opencode-project: <project id>
x-opencode-session: <affinity>
x-opencode-client: <client name>
```

Provider authentication may add its bearer/auth headers. Custom `request.http.headers` are merged into the request in `packages/ai/src/route/transport/http.ts:45-63`.

## Body construction

`OpenAIChat.fromRequest` builds the body (`packages/ai/src/protocols/openai-chat.ts:812-855`). Undefined object properties are omitted by JSON encoding; the body schema is defined at `packages/ai/src/protocols/openai-chat.ts:175-195`.

### Fields

| JSON field | Set when | Wire value |
|---|---|---|
| `model` | Always | `request.model.id` |
| `messages` | Always | Lowered OpenAI Chat messages; system/user/assistant/tool roles. User content can be string or text/image/file parts. See `openai-chat.ts:118-164`. |
| `tools` | Active tools exist | Array of `{type:"function", function:{name, description, parameters, strict?}, cache_control?}`. If no active tools but tool history exists, sends `[]`; otherwise omitted (`openai-chat.ts:835-840`). |
| `tool_choice` | Active tools and common `request.toolChoice` exist | `"auto"`, `"none"`, `"required"`, or `{type:"function",function:{name}}` (`openai-chat.ts:167-173,841`). |
| `stream` | Always | `true` |
| `stream_options` | `supportsUsageInStreaming` is true; default detector is true | `{include_usage:true}` (`openai-chat.ts:759,823-824,842-844`) |
| `max_completion_tokens` | Model/provider selects the modern field | `generation.maxTokens` |
| `max_tokens` | Model/provider selects the legacy field | `generation.maxTokens`; exactly one token-limit field is selected (`openai-chat.ts:683-715,820-847`) |
| `temperature` | `generation.temperature` is defined | Number |
| `top_p` | `generation.topP` is defined | Number |
| `frequency_penalty` | `generation.frequencyPenalty` is defined | Number |
| `presence_penalty` | `generation.presencePenalty` is defined | Number |
| `seed` | `generation.seed` is defined | Number |
| `stop` | `generation.stop` is defined | String array |
| `store` | Provider supports storage; explicit value wins, otherwise sends `false` | Boolean (`openai-chat.ts:717-757,lowerOptions at 786-804`) |
| `prompt_cache_key` | Prompt cache is enabled, a key exists, and provider compatibility allows it | Clamped to 64 Unicode characters (`packages/ai/src/protocols/shared.ts:45-66`; `openai-chat.ts:786-804`) |
| `reasoning_effort` | `providerOptions.reasoningEffort` resolves to a truthy value | Provider-compatible effort string (`openai-chat.ts:786-804`) |
| `tool_stream` | Z.ai-compatible provider/model with active tools | `true` (`openai-chat.ts:827-845`) |

`max_completion_tokens` versus `max_tokens` is provider-dependent. The default detector chooses `max_completion_tokens` for OpenAI-compatible endpoints except known legacy providers; an explicit model compatibility setting overrides detection (`openai-chat.ts:683-715,820-847`).

### Reasoning fields

There is no top-level `reasoning` request option except `reasoning_effort`. Reasoning is carried in replayed assistant messages. The accepted assistant fields are:

```json
{
  "role": "assistant",
  "content": "answer or null",
  "reasoning_content": "provider-native reasoning text",
  "reasoning": "provider-native reasoning text",
  "reasoning_text": "provider-native reasoning text",
  "reasoning_details": [
    {"type":"reasoning.text","text":"..."},
    {"type":"reasoning.summary","summary":"..."},
    {"type":"reasoning.encrypted","data":"..."},
    {"type":"summary","summary":"..."},
    {"type":"encrypted","encrypted":"..."}
  ]
}
```

The accepted message schema and reasoning-detail dialects are at `packages/ai/src/protocols/openai-chat.ts:80-116,138-164`. Provider metadata can select a custom reasoning field; reserved fields (`role`, `content`, `refusal`, `tool_calls`) are rejected (`openai-chat.ts:33,799-810`). Reasoning chunks are read from the configured field, then `reasoning_content`, `reasoning`, or `reasoning_text` (`openai-chat.ts:944-960`).

### Cache fields

There are two distinct cache mechanisms:

1. **Body affinity key:** `prompt_cache_key`, only when the request has a cache key and compatibility permits it. The helper disables it when `request.cache === "none"` and clamps it to 64 Unicode characters (`packages/ai/src/protocols/shared.ts:45-66`).
2. **Provider cache markers:** when the model declares a `cacheFormat`, tools and text content may receive `cache_control: {type:"ephemeral", ttl?: string}`. Tool lowering adds the marker at `openai-chat.ts:334-343`; the marker schema is `openai-chat.ts:43-47,118-123`. The default declared marker is enabled only when `cacheFormat` is declared (`openai-chat.ts:858-860`).

No generic `cache_read`, `cache_write`, or `cached_tokens` request fields are sent. Those names are response-usage accounting fields; accepted response fields are defined at `openai-chat.ts:214-247` and mapped at `openai-chat.ts:925-940`.

## Example request bodies

### Normal agent turn with tools

This shows the standard OpenAI-compatible shape. Values such as model, messages, token budget, and tool set are runtime values.

```json
{
  "model": "gpt-4o",
  "messages": [
    {"role": "system", "content": "You are an AI agent running in OpenCode."},
    {"role": "user", "content": "Read README.md and summarize it."}
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "read",
        "description": "Read the contents of a file or directory.",
        "parameters": {
          "type": "object",
          "properties": {
            "path": {"type": "string"},
            "offset": {"type": "integer", "minimum": 0},
            "limit": {"type": "integer", "minimum": 0}
          },
          "required": ["path"],
          "additionalProperties": false
        },
        "strict": false
      }
    }
  ],
  "stream": true,
  "stream_options": {"include_usage": true},
  "store": false,
  "prompt_cache_key": "<session affinity>",
  "max_completion_tokens": 32000,
  "temperature": 0
}
```

`strict:false` is emitted for providers detected as strict-compatible (`openai-chat.ts:825-826,334-343`). Providers detected as Moonshot, Together, Cloudflare AI Gateway, or NVIDIA omit that property.

### Minimal turn

```json
{
  "model": "gpt-4o-mini",
  "messages": [{"role":"user","content":"Say hello."}],
  "stream": true,
  "stream_options": {"include_usage": true},
  "max_completion_tokens": 20,
  "temperature": 0
}
```

The recording fixture shows the same shape plus `store:false` and `prompt_cache_key` when those capabilities are enabled (`packages/core/test/fixtures/recordings/session-runner/openai-chat-streams-text.json:12-16`).

## Builtin tools

The current source calls the shell tool `shell`, not `bash`. V1 migration explicitly renames `bash` to `shell` (`packages/core/src/database/v1-migration.bun.ts:899-901`). A reimplementation accepting the historical name should normalize `bash` to `shell`; the current request's builtin name is `shell`.

The registry installs these builtin plugins (`packages/core/src/plugin/internal.ts:215-245`). The snapshot emits direct tools sorted by effective name, then the `execute` Code Mode tool (`packages/core/src/tool.ts:250-265`). `definition()` converts each tool's declared input schema into JSON Schema and emits `{type:"tool", name, description, inputSchema}` (`packages/core/src/tool/runtime.ts:21-26,156-161`). Thus the OpenAI adapter wraps each definition as `type:"function"` (`openai-chat.ts:334-343`).

### Complete current builtin name list

Direct tools normally available to a primary agent:

- `edit` — exact text replacement in a file; edit permission required.
- `glob` — search file paths using a glob pattern.
- `grep` — search file contents with ripgrep regex or literal matching.
- `patch` — apply add/update/delete patch text; edit permission required.
- `question` — ask the user questions during execution.
- `read` — read files or directories, including images and PDFs.
- `shell` — execute a shell command and return its output. Historical name: `bash`.
- `skill` — load a specialized skill's instructions and resources.
- `subagent` — spawn an agent in a child session.
- `webfetch` — fetch HTTP/HTTPS content as text, Markdown, or HTML.
- `websearch` — search the web using the selected search integration.
- `write` — write/overwrite a file; edit permission required.

When Code Mode is enabled, the direct definitions are generally placed in the catalog and the model-facing request contains the additional `execute` tool. Namespace tools under `opencode` and MCP resource tools are Code Mode tools, not additional direct OpenAI function definitions (`packages/core/src/tool/plugin/opencode.ts:102-225`; `packages/core/src/tool/plugin/mcp-resource.ts:15-65`). MCP server tools can additionally be registered dynamically.

### `shell` / historical `bash` definition — complete

Source: `packages/core/src/tool/plugin/shell.ts:23-61,191-198`.

Description (the default shell-specific text is resolved at runtime):

```text
Execute a shell command and return its output. Quote file paths containing spaces or special characters. Prefer dedicated tools over shell commands when possible. When output is large, the full result is saved to a file and a truncated preview is returned. Rely on automatic truncation unless filtering the output is more useful. Commands accept an optional timeout, background commands have no timeout by default. Background commands return immediately, and you will be notified when they complete.
```

If a shell is resolved, the second sentence is inserted after the first: `Commands run on <OS> using <shell>.` (`shell.ts:28-46`).

Exact input JSON Schema shape (the schema converter may add standard JSON Schema metadata):

```json
{
  "type": "object",
  "properties": {
    "command": {
      "type": "string",
      "description": "Shell command string to execute"
    },
    "workdir": {
      "type": "string",
      "description": "Working directory to execute the command in. Defaults to the current working directory. When possible, avoid changing directories in the command and set the working directory here instead."
    },
    "timeout": {
      "type": "integer",
      "minimum": 0,
      "description": "Timeout in milliseconds. Set to 0 to disable the timeout. Defaults to 120000 for foreground commands. Background commands have no timeout by default."
    },
    "background": {
      "type": "boolean",
      "description": "Run the command in the background and return immediately (useful for dev servers and long-running builds). You do not need to use '&' at the end of the command when using this parameter. You will be notified when it completes. DO NOT poll for completion."
    }
  },
  "required": ["command"],
  "additionalProperties": false
}
```

The `timeout` default in the source is `2 * 60 * 1000` (`shell.ts:23-24,54-55`). The tool's output schema is not sent in OpenAI `parameters`; only the input schema is used by `lowerTool`.

### `read` definition — complete

Source: `packages/core/src/tool/plugin/read.ts:15-27,40-46`.

Exact description:

```text
Read the contents of a file or directory. Supports text files, images, and PDFs. Images and PDFs are presented directly to the model. Each text line is prefixed by its 1-based line number as <line>: <content>. The prefix is for reference and is not part of the file content. Directory entries are returned one per line. Use offset and limit to read large files or directories in sections. Prefer one larger read over many small slices, and use grep to find specific content in large files.
```

Input schema:

```json
{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "description": "File or directory to read"
    },
    "offset": {
      "type": "integer",
      "minimum": 0,
      "description": "The line or directory entry to start reading from (1-based)"
    },
    "limit": {
      "type": "integer",
      "minimum": 0,
      "description": "The maximum number of lines or directory entries to read (defaults to and capped at 2000)"
    }
  },
  "required": ["path"],
  "additionalProperties": false
}
```

`offset` and `limit` are optional non-negative integers (`packages/core/src/tool/read-filesystem.ts:69-74`). The read output is a union of file, text-page, and directory-page results, but output schemas are not included in OpenAI function `parameters`.

### Other builtin input schemas and descriptions

These are the model-facing definitions from the plugin registration. `parameters` is the JSON Schema generated from each listed input schema; all optional properties are omitted from `required`, and the schema converter rejects additional properties (`packages/core/src/tool/runtime.ts:156-161`).

| Name | Description | Input properties |
|---|---|---|
| `edit` | `Edit the contents of a file by finding and replacing exact text. When editing text from Read output, preserve the exact indentation (tabs or spaces) and omit the line-number prefix, such as \`1: \`. Never include the prefix in oldString or newString. Write line breaks as \\n; the file's existing CRLF or LF line endings are matched and preserved. To change line endings themselves, make oldString and newString differ only in \\r; that edit applies to the exact bytes. If there is no exact match, the edit retries while ignoring typographic quote, dash, and space variants, then trailing whitespace on each line. The edit fails if oldString is not found. By default, oldString must identify a UNIQUE location. Multiple matches FAIL unless replaceAll is true. Add more surrounding context to disambiguate, or set replaceAll to true to replace every occurrence. Use replaceAll when the change should apply to every occurrence, such as renaming a variable.` (`edit.ts:219-226) | `path:string` required; `oldString:string` required; `newString:string` required; `replaceAll:boolean` optional |
| `glob` | `Search file paths using a glob pattern (examples: "**/*.ts", "src/**/*.tsx").` (`glob.ts:56-63) | `pattern:string` required; `path:string` optional; `hidden:boolean` optional; `limit:integer > 0` optional |
| `grep` | `Search file contents using ripgrep's regular expression syntax or literal text matching. Use it to locate specific code, symbols, or text patterns, and narrow searches with \`path\` or \`include\`. Returns matching file paths, line numbers, and line previews.` (`grep.ts:75-83) | `pattern:string` required, min length 1; `path:string` optional; `include:string` optional; `literal:boolean` optional; `caseSensitive:boolean` optional; `limit:integer > 0` optional |
| `patch` | The full patch text describing add, update, and delete operations (`patch.ts:21-24,79-87) | `patchText:string` required |
| `question` | `Use this tool when you need to ask the user questions during execution. This allows you to: 1. Gather user preferences or requirements 2. Clarify ambiguous instructions 3. Get decisions on implementation choices as you work 4. Offer choices to the user about what direction to take.` It also includes the usage notes in `question.ts:12-20`. | `questions:array` required and non-empty; each item is `Question.Prompt` |
| `skill` | `Load a specialized skill's instructions and resources into the current conversation when the task at hand matches its description.` Then: `The skill ID must match an available skill or a skill explicitly referenced by the user.` (`skill.ts:21-25) | `id:string` required |
| `subagent` | `Spawns an agent in a child session to work on the specified task.` Plus the foreground/background/session guidance in `subagent.ts:55-63`. | `agent:string` required; `description:string` required; `prompt:string` required; `model:string` optional; `sessionID:string` optional; `background:boolean` optional |
| `webfetch` | `Fetch content from an HTTP or HTTPS URL and return it as text, markdown, or HTML. Markdown is the default.` Large-result guidance follows (`webfetch.ts:17-21). | `url:string` required; `format:"text"|"markdown"|"html"` optional with default `"markdown"`; `timeout:number` optional, >0 and <=120 |
| `websearch` | `Search the web using the user's selected search integration. Use this for current information beyond knowledge cutoff.` Current-year guidance follows (`websearch.ts:20-22). | `query:string` required |
| `write` | `Writes a file to the local filesystem, overwriting if one exists.\n\nMissing parent directories are created automatically.\n\nUse this tool to create new files or overwrite existing files. For partial changes, use the edit tool instead.` (`write.ts:57-64) | `path:string` required; `content:string` required |

`opencode` namespace tools are registered separately with `options.namespace:"opencode", codemode:true`; they are not normal direct function entries when Code Mode is active (`packages/core/src/tool/plugin/opencode.ts:106-225`).

## Streaming / SSE

The OpenAI Chat route uses `Framing.sseWithDone` (`packages/ai/src/protocols/openai-chat.ts:1309-1310`). The framing layer:

1. Decodes response bytes as UTF-8.
2. Feeds them to the Effect SSE parser, which handles SSE event boundaries and `data:` fields; it does not require each network chunk to contain a complete line.
3. Emits each non-empty event's `event.data` payload.
4. Drops `data: null`, `data: : keepalive`, empty events, and parser errors become `AIError`.
5. Retains `data: [DONE]` because this route uses `sseWithDone` (`packages/ai/src/route/framing.ts:28-78,83-87`).

Each retained frame is decoded as either the literal string `[DONE]` or JSON matching the OpenAI event schema (`packages/ai/src/protocols/openai-chat.ts:282-292`; `packages/ai/src/route/protocol.ts:145-155`). The protocol returns no events for `[DONE]` and treats it as terminal (`openai-chat.ts:1303-1305`).

Typical wire response:

```text
data: {"choices":[{"delta":{"role":"assistant","content":"Hello"},"finish_reason":null}],"usage":null}

data: {"choices":[{"delta":{"content":"!"},"finish_reason":null}],"usage":null}

data: {"choices":[{"delta":{},"finish_reason":"stop"}],"usage":null}

data: {"choices":[],"usage":{"prompt_tokens":465,"completion_tokens":2,"total_tokens":467,"prompt_tokens_details":{"cached_tokens":0},"completion_tokens_details":{"reasoning_tokens":0}}}

data: [DONE]
```

The recording confirms the usage-only final event followed by `[DONE]` (`packages/core/test/fixtures/recordings/session-runner/openai-chat-streams-text.json:16`). Tool-call argument fragments are accumulated across deltas; the protocol emits the completed tool call after the stream state has enough JSON (`openai-chat.ts:295-319` and the tool-call delta schema at `openai-chat.ts:250-278`).

### Usage

The request asks for usage with:

```json
"stream_options": {"include_usage": true}
```

When supported, the server sends a final chunk with `choices:[]` and `usage`. OpenCode maps:

- `prompt_tokens` -> inclusive input tokens
- `completion_tokens` -> output tokens
- `total_tokens` -> total
- `prompt_tokens_details.cached_tokens`, `prompt_cache_hit_tokens`, top-level `cached_tokens`, or `cache_read_input_tokens` -> cache-read tokens
- `prompt_tokens_details.cache_write_tokens` or `cache_created_input_tokens` -> cache-write tokens
- `completion_tokens_details.reasoning_tokens` -> reasoning tokens

The exact mapping and provider metadata preservation are in `packages/ai/src/protocols/openai-chat.ts:925-940`. If a provider compatibility setting disables streaming usage, `stream_options` is omitted (`openai-chat.ts:823-844`).

## Compression

Compression is request-body compression, not response-stream compression. `request.http.encoding` controls it (`packages/ai/src/route/transport/http.ts:91-101`).

- `undefined` or `"none"`: plain UTF-8 JSON body; no `content-encoding`.
- `"zstd"`: call `node:zlib.zstdCompressSync(text)` when available; set `content-encoding: zstd`.
- `"gzip"`: call `node:zlib.gzipSync(text)`; set `content-encoding: gzip`.
- Missing runtime support or compression failure: silently fall back to the plain JSON body (`packages/ai/src/protocols/shared.ts:396-435`).

The compressed request still has `content-type: application/json`; compressed bytes are sent with `bodyUint8Array` (`shared.ts:408-415`). The ChatGPT/OpenAI provider plugin defaults ChatGPT-hosted requests to zstd only when the base URL was not custom-configured; a custom base URL does not receive that default (`packages/core/src/plugin/provider/openai.ts:257-268`; analogous ChatGPT plugin behavior at `packages/core/src/plugin/provider/chatgpt.ts:284-296`). For the explicitly requested `https://opencode.ai/inference/openai/v1` deployment, use the configured `request.http.encoding`; do not assume zstd from the generic OpenAI route.
