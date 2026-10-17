use bytes::Bytes;
use futures_util::{stream, Stream, StreamExt};
use rand::Rng;
use reqwest::{Client, Response};
use serde::Serialize;
use serde_json::Value;
use std::error::Error;
use std::pin::Pin;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const MODELS_ENDPOINT: &str = "https://opencode.ai/inference/v1/models";
pub const CHAT_ENDPOINT: &str = "https://opencode.ai/inference/openai/v1/chat/completions";
const FALLBACK_VERSION: &str = "1.19.3";
pub const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

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
}

#[derive(Debug, Serialize)]
struct ChatBody<'a> {
    model: &'a str,
    messages: &'a [Value],
    tools: &'a [Value],
    tool_choice: &'static str,
    stream: bool,
    stream_options: StreamOptions,
    max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, serde::Deserialize)]
struct ModelsResponse {
    data: Vec<Model>,
}

#[derive(Debug, serde::Deserialize)]
struct Model {
    id: String,
}

static SESSION_STATE: OnceLock<Mutex<(u64, u64)>> = OnceLock::new();

fn user_agent() -> String {
    format!("opencode/latest/{FALLBACK_VERSION}/cli")
}

pub fn session_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_millis() as u64;
    let state = SESSION_STATE.get_or_init(|| Mutex::new((0, 0)));
    let counter = {
        let mut state = state.lock().expect("session ID state poisoned");
        if state.0 != timestamp {
            state.0 = timestamp;
            state.1 = 0;
        }
        state.1 += 1;
        state.1
    };

    let value = !(timestamp.wrapping_mul(0x1000).wrapping_add(counter));
    let mut id = String::from("ses_");
    for shift in [40, 32, 24, 16, 8, 0] {
        id.push_str(&format!("{:02x}", (value >> shift) & 0xff));
    }

    let mut random = [0u8; 14];
    rand::rng().fill(&mut random);
    for byte in random {
        id.push(ALPHABET[(byte % 62) as usize] as char);
    }
    id
}

pub async fn list_models() -> Result<Vec<String>> {
    let response = Client::new().get(MODELS_ENDPOINT).send().await?;
    let response = response.error_for_status()?;
    Ok(response
        .json::<ModelsResponse>()
        .await?
        .data
        .into_iter()
        .map(|model| model.id)
        .collect())
}

pub async fn chat_stream(request: ChatRequest) -> Result<ChatStream> {
    send_chat_stream(&request).await
}

pub async fn chat_stream_with_retry(request: ChatRequest) -> Result<ChatStream> {
    const BACKOFFS: [Duration; 4] = [
        Duration::from_secs(5),
        Duration::from_secs(15),
        Duration::from_secs(30),
        Duration::ZERO,
    ];

    for (attempt, backoff) in BACKOFFS.iter().enumerate() {
        match send_chat_stream(&request).await {
            Ok(stream) => return Ok(stream),
            Err(error) => {
                let retry = error
                    .downcast_ref::<UpstreamError>()
                    .is_some_and(|error| error.status == 429 || error.status == 403);
                if !retry || attempt == 3 {
                    return Err(error);
                }
                tokio::time::sleep(*backoff).await;
            }
        }
    }
    unreachable!("retry loop always returns")
}

async fn send_chat_stream(request: &ChatRequest) -> Result<ChatStream> {
    let body = ChatBody {
        model: &request.model,
        messages: &request.messages,
        tools: &request.tools,
        tool_choice: "auto",
        stream: true,
        stream_options: StreamOptions {
            include_usage: true,
        },
        max_tokens: request.max_tokens,
    };
    let response = Client::new()
        .post(CHAT_ENDPOINT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, user_agent())
        .header("x-opencode-session", session_id())
        .json(&body)
        .send()
        .await?;
    ensure_success(response).await
}

async fn ensure_success(response: Response) -> Result<ChatStream> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        return Err(UpstreamError {
            status: status.as_u16(),
            body,
        }
        .into());
    }
    Ok(Box::pin(parse_sse_stream(response.bytes_stream())))
}

pub(crate) fn parse_sse_stream<S, E>(input: S) -> impl Stream<Item = Result<Value>>
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
        |mut state| async move {
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
                    return Some((serde_json::from_str(data).map_err(Into::into), state));
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
                    return Some((serde_json::from_str(data).map_err(Into::into), state));
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
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    #[tokio::test]
    async fn session_ids_have_expected_shape_and_differ() {
        let first = session_id();
        let second = session_id();
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
        let chunks: Vec<_> = parse_sse_stream(input)
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

    #[tokio::test]
    async fn user_agent_uses_verified_fingerprint() {
        assert_eq!(user_agent(), "opencode/latest/1.19.3/cli");
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
        };
        // The free tier flaps under load (429/403 on identical verified requests), so retry.
        let mut stream = None;
        for attempt in 0..6 {
            match chat_stream(request.clone()).await {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) if attempt < 5 => {
                    eprintln!("upstream rejected (attempt {attempt}): {e}; retrying");
                    tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                }
                Err(e) => panic!("upstream never accepted: {e}"),
            }
        }
        let mut stream = stream.unwrap();
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
