use bytes::Bytes;
use futures_util::{stream, Stream, StreamExt};
mod catalog;
mod protocol;
mod reasoning;
mod session;
mod tools;
mod version;

pub use catalog::ModelInfo;
pub use protocol::Protocol;
pub use tools::with_required_tools;

use reqwest::{Client, Response};
use serde_json::Value;
use std::error::Error;
use std::pin::Pin;

pub const MODELS_ENDPOINT: &str = "https://opencode.ai/inference/v1/models";

fn endpoint_url(provider: crate::config::Provider, protocol: Protocol) -> &'static str {
    match (provider, protocol) {
        (crate::config::Provider::Zen, Protocol::Chat) => {
            "https://opencode.ai/inference/openai/v1/chat/completions"
        }
        (crate::config::Provider::Zen, Protocol::Responses) => {
            "https://opencode.ai/inference/openai/v1/responses"
        }
        (crate::config::Provider::Zen, Protocol::Messages) => {
            "https://opencode.ai/inference/anthropic/v1/messages"
        }
        (crate::config::Provider::Go, Protocol::Chat) => {
            "https://opencode.ai/inference/go/openai/v1/chat/completions"
        }
        (crate::config::Provider::Go, Protocol::Responses) => {
            "https://opencode.ai/inference/go/openai/v1/responses"
        }
        (crate::config::Provider::Go, Protocol::Messages) => {
            "https://opencode.ai/inference/go/anthropic/v1/messages"
        }
    }
}

pub type BoxError = Box<dyn Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, BoxError>;
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<Value>> + Send>>;

#[derive(Debug)]
pub struct UpstreamError {
    pub status: u16,
    pub body: String,
}

impl std::fmt::Display for UpstreamError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "OpenCode request failed ({}): {}",
            self.status, self.body
        )
    }
}

impl Error for UpstreamError {}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Value>,
    pub tools: Vec<Value>,
    pub max_tokens: u32,
    pub provider: crate::config::Provider,
    pub protocol: Protocol,
    pub api_key: Option<String>,
}

pub async fn list_models(config: &crate::config::RuntimeConfig) -> Result<Vec<ModelInfo>> {
    Ok(catalog::fetch(config).await?.into_values().collect())
}

pub async fn chat_stream(request: ChatRequest) -> Result<ChatStream> {
    send_chat_stream(&request).await
}

async fn send_chat_stream(request: &ChatRequest) -> Result<ChatStream> {
    let protocol = request.protocol;
    let body = protocol.request_body(request);
    let session = session::proxy_session_id();
    let endpoint = endpoint_url(request.provider, request.protocol);
    let response = Client::new()
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .header(
            reqwest::header::AUTHORIZATION,
            request
                .api_key
                .as_deref()
                .map_or("Bearer public".to_string(), |key| format!("Bearer {key}")),
        )
        .header(reqwest::header::USER_AGENT, version::user_agent().await)
        .header("x-opencode-client", "cli")
        .header("x-opencode-project", "global")
        .header("x-opencode-session", session)
        .header("x-opencode-session-id", session)
        .header("x-session-affinity", session)
        .header("x-session-id", session)
        .json(&body)
        .send()
        .await?;
    tracing::debug!(model = %request.model, "upstream request sent");
    ensure_success(response, protocol).await
}

async fn ensure_success(response: Response, protocol: Protocol) -> Result<ChatStream> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        return Err(UpstreamError {
            status: status.as_u16(),
            body,
        }
        .into());
    }
    Ok(Box::pin(parse_sse_stream(
        response.bytes_stream(),
        protocol,
    )))
}

pub(crate) fn parse_sse_stream<S, E>(
    input: S,
    protocol: Protocol,
) -> impl Stream<Item = Result<Value>>
where
    S: Stream<Item = std::result::Result<Bytes, E>> + Send + 'static,
    E: Error + Send + Sync + 'static,
{
    struct State<S> {
        input: Pin<Box<S>>,
        buffer: String,
        done: bool,
    }

    stream::unfold(
        State {
            input: Box::pin(input),
            buffer: String::new(),
            done: false,
        },
        move |mut state| {
            let protocol = protocol;
            async move {
                loop {
                    if let Some(newline) = state.buffer.find('\n') {
                        let line = state.buffer[..newline].trim_end_matches('\r').to_string();
                        state.buffer.drain(..=newline);
                        let Some(data) = line.strip_prefix("data:").map(str::trim_start) else {
                            continue;
                        };
                        if data == "[DONE]" {
                            return None;
                        }
                        let parsed = serde_json::from_str::<Value>(data)
                            .map_err(Into::into)
                            .and_then(|chunk: Value| {
                                protocol
                                    .normalize_chunk(chunk)
                                    .ok_or_else(|| "unsupported upstream event".into())
                            });
                        return Some((parsed, state));
                    }
                    if state.done {
                        if state.buffer.is_empty() {
                            return None;
                        }
                        let line = std::mem::take(&mut state.buffer);
                        let data = line.strip_prefix("data:").map(str::trim_start)?;
                        if data == "[DONE]" {
                            return None;
                        }
                        let parsed = serde_json::from_str::<Value>(data)
                            .map_err(Into::into)
                            .and_then(|chunk: Value| {
                                protocol
                                    .normalize_chunk(chunk)
                                    .ok_or_else(|| "unsupported upstream event".into())
                            });
                        return Some((parsed, state));
                    }
                    match state.input.next().await {
                        Some(Ok(bytes)) => state.buffer.push_str(&String::from_utf8_lossy(&bytes)),
                        Some(Err(error)) => {
                            state.done = true;
                            return Some((Err(error.into()), state));
                        }
                        None => state.done = true,
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    #[tokio::test]
    async fn session_ids_have_expected_shape_and_differ() {
        let first = session::session_id();
        let second = session::session_id();
        assert!(first.starts_with("ses_"));
        assert_eq!(first.len(), 30);
        assert!(first[4..16]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
        assert!(first[16..].bytes().all(|byte| byte.is_ascii_alphanumeric()));
        assert_ne!(first, second);
    }

    #[tokio::test]
    async fn sse_parser_extracts_chunks_until_done() {
        let input = stream::iter(vec![
            Ok::<Bytes, std::io::Error>(Bytes::from_static(b": comment\n\ndata: {")),
            Ok(Bytes::from_static(b"\"id\":1}\n")),
            Ok(Bytes::from_static(b"data: {\"id\":2}\n\n")),
            Ok(Bytes::from_static(b"data: [DONE]\ndata: {\"id\":3}\n")),
        ]);
        let chunks: Vec<_> = parse_sse_stream(input, Protocol::Chat)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            chunks,
            vec![serde_json::json!({"id": 1}), serde_json::json!({"id": 2})]
        );
    }

    #[test]
    fn latest_version_requires_a_v2_semver() {
        assert!(version::is_supported("2.0.26"));
        assert!(version::is_supported("2.1.3"));
        assert!(!version::is_supported("1.19.3"));
        assert!(!version::is_supported("latest"));
    }

    #[tokio::test]
    #[ignore]
    async fn live_chat_stream_returns_content() {
        let request = ChatRequest {
            model: "glm-5.3-flash-free".into(),
            messages: vec![serde_json::json!({"role":"user","content":"Reply with exactly OK"})],
            tools: vec![
                serde_json::json!({"type":"function","function":{"name":"bash","description":"Execute a shell command","parameters":{"type":"object","properties":{"command":{"type":"string"}}}}}),
                serde_json::json!({"type":"function","function":{"name":"read","description":"Read a file","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}}),
            ],
            max_tokens: 16,
            provider: crate::config::Provider::Zen,
            protocol: Protocol::Chat,
            api_key: None,
        };
        let mut stream = chat_stream(request).await.unwrap();
        let mut found = false;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            found |= chunk["choices"][0]["delta"]["content"]
                .as_str()
                .is_some_and(|content| !content.is_empty());
        }
        assert!(found);
    }
}
