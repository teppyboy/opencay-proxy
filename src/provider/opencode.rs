use rand::Rng;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::io::{self, BufRead, BufReader, Read};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MODELS_ENDPOINT: &str = "https://opencode.ai/inference/v1/models";
pub const CHAT_ENDPOINT: &str = "https://opencode.ai/inference/openai/v1/chat/completions";
const RELEASES_ENDPOINT: &str = "https://api.github.com/repos/anomalyco/opencode/releases/latest";
// Last version verified against the free-tier gate; used if the release lookup fails.
const FALLBACK_VERSION: &str = "1.19.3";
pub const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

type BoxError = Box<dyn Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, BoxError>;

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<Value>,
    pub max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ChatBody<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
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

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<Model>,
}

#[derive(Debug, Deserialize)]
struct Model {
    id: String,
}

static SESSION_STATE: OnceLock<Mutex<(u64, u64)>> = OnceLock::new();
static CACHED_VERSION: OnceLock<String> = OnceLock::new();

fn user_agent() -> String {
    let version = CACHED_VERSION.get_or_init(|| {
        #[derive(Deserialize)]
        struct Release {
            tag_name: String,
        }
        Client::new()
            .get(RELEASES_ENDPOINT)
            .header(reqwest::header::USER_AGENT, "opencay-proxy")
            .send()
            .and_then(Response::error_for_status)
            .and_then(|response| response.json::<Release>())
            .ok()
            .map(|release| release.tag_name.trim_start_matches('v').to_string())
            .filter(|version| !version.is_empty())
            .unwrap_or_else(|| FALLBACK_VERSION.to_string())
    });
    format!("opencode/latest/{version}/cli")
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

pub fn list_models() -> Result<Vec<String>> {
    let response = Client::new().get(MODELS_ENDPOINT).send()?;
    let response = response.error_for_status()?;
    Ok(response
        .json::<ModelsResponse>()?
        .data
        .into_iter()
        .map(|model| model.id)
        .collect())
}

pub fn chat_stream(request: ChatRequest) -> Result<ChatStream> {
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
        .send()?;
    ensure_success(response)
}

pub struct ChatStream {
    parser: SseParser<BufReader<Response>>,
}

impl Iterator for ChatStream {
    type Item = Result<Value>;

    fn next(&mut self) -> Option<Self::Item> {
        self.parser.next()
    }
}

fn ensure_success(mut response: Response) -> Result<ChatStream> {
    if !response.status().is_success() {
        let status = response.status();
        let mut body = String::new();
        response.read_to_string(&mut body)?;
        return Err(io::Error::other(format!("OpenCode request failed ({status}): {body}")).into());
    }
    Ok(ChatStream {
        parser: SseParser::new(BufReader::new(response)),
    })
}

struct SseParser<R> {
    reader: R,
    done: bool,
}

impl<R: BufRead> SseParser<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            done: false,
        }
    }
}

impl<R: BufRead> Iterator for SseParser<R> {
    type Item = Result<Value>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let mut line = String::new();
        loop {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => {
                    self.done = true;
                    return None;
                }
                Ok(_) => {}
                Err(error) => {
                    self.done = true;
                    return Some(Err(error.into()));
                }
            }
            let line = line.trim_end_matches(['\r', '\n']);
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim_start();
            if data == "[DONE]" {
                self.done = true;
                return None;
            }
            return Some(serde_json::from_str(data).map_err(Into::into));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn session_ids_have_expected_shape_and_differ() {
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

    #[test]
    fn sse_parser_extracts_chunks_until_done() {
        let input = b": comment\n\ndata: {".as_slice();
        let input = [
            input,
            b"\"id\":1}\n".as_slice(),
            b"data: {\"id\":2}\n\n".as_slice(),
            b"data: [DONE]\n".as_slice(),
            b"data: {\"id\":3}\n".as_slice(),
        ]
        .concat();
        let chunks: Vec<_> = SseParser::new(Cursor::new(input))
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            chunks,
            vec![serde_json::json!({"id": 1}), serde_json::json!({"id": 2})]
        );
    }

    #[test]
    fn user_agent_uses_fetched_or_fallback_version() {
        let ua = user_agent();
        assert!(ua.starts_with("opencode/latest/"));
        assert!(ua.ends_with("/cli"));
    }

    #[test]
    #[ignore]
    fn live_chat_stream_returns_content() {
        let tools = vec![
            serde_json::json!({"type":"function","function":{"name":"bash","description":"Execute a shell command","parameters":{"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}}}),
            serde_json::json!({"type":"function","function":{"name":"read","description":"Read a file","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}),
        ];
        let request = ChatRequest {
            model: "glm-5.3-flash-free".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "Reply with exactly OK".into(),
            }],
            tools,
            max_tokens: 16,
        };
        let chunks: Vec<_> = chat_stream(request)
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert!(chunks.iter().any(|chunk| {
            chunk["choices"][0]["delta"]["content"]
                .as_str()
                .is_some_and(|content| !content.is_empty())
        }));
    }
}
