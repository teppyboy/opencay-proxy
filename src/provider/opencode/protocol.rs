use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Chat,
    Responses,
    Messages,
}

impl Protocol {
    pub fn for_model(model: &str) -> Self {
        let base = model.split('(').next().unwrap_or(model).trim();
        if base == "union-alpha" {
            Self::Messages
        } else if base.starts_with("muse-spark") {
            Self::Responses
        } else {
            Self::Chat
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            Self::Chat => "https://opencode.ai/inference/v1/chat/completions",
            Self::Responses => "https://opencode.ai/inference/v1/responses",
            Self::Messages => "https://opencode.ai/inference/v1/messages",
        }
    }

    pub fn request_body(self, request: &crate::provider::opencode::ChatRequest) -> Value {
        let messages =
            crate::provider::opencode::reasoning::with_reasoning_content(&request.messages);
        match self {
            Self::Chat => json!({
                "model": request.model,
                "messages": messages,
                "tools": request.tools,
                "tool_choice": "auto",
                "stream": true,
                "stream_options": {"include_usage": true},
                "max_tokens": request.max_tokens,
            }),
            Self::Responses => json!({
                "model": request.model,
                "input": responses_input(&messages),
                "tools": responses_tools(&request.tools),
                "tool_choice": "auto",
                "stream": true,
                "store": false,
                "max_output_tokens": request.max_tokens,
            }),
            Self::Messages => json!({
                "model": request.model,
                "system": system_text(&messages),
                "messages": messages_to_anthropic(&messages),
                "tools": messages_tools(&request.tools),
                "max_tokens": request.max_tokens,
                "stream": true,
            }),
        }
    }

    pub fn normalize_chunk(self, chunk: Value) -> Option<Value> {
        match self {
            Self::Chat => Some(chunk),
            Self::Responses => normalize_responses_chunk(chunk),
            Self::Messages => normalize_messages_chunk(chunk),
        }
    }
}

fn responses_input(messages: &[Value]) -> Vec<Value> {
    messages.iter().flat_map(|message| {
        let role = message["role"].as_str().unwrap_or("user");
        let content = message["content"].as_str().unwrap_or("");
        let mut items = Vec::new();
        if role != "system" {
            items.push(json!({
                "type": "message",
                "role": if role == "assistant" { "assistant" } else { "user" },
                "content": [{"type": if role == "assistant" { "output_text" } else { "input_text" }, "text": content}],
            }));
        }
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                items.push(json!({
                    "type": "function_call",
                    "call_id": call["id"],
                    "name": call["function"]["name"],
                    "arguments": call["function"]["arguments"],
                }));
            }
        }
        if role == "tool" {
            items.push(json!({
                "type": "function_call_output",
                "call_id": message["tool_call_id"],
                "output": content,
            }));
        }
        items
    }).collect()
}

fn messages_to_anthropic(messages: &[Value]) -> Vec<Value> {
    messages.iter().filter_map(|message| {
        let role = message["role"].as_str()?;
        if role == "system" { return None; }
        let role = if role == "assistant" { "assistant" } else { "user" };
        let mut content = Vec::new();
        if let Some(text) = message["content"].as_str() {
            if !text.is_empty() { content.push(json!({"type": "text", "text": text})); }
        }
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                content.push(json!({
                    "type": "tool_use",
                    "id": call["id"],
                    "name": call["function"]["name"],
                    "input": serde_json::from_str::<Value>(call["function"]["arguments"].as_str().unwrap_or("{}")).unwrap_or_else(|_| json!({})),
                }));
            }
        }
        if role == "user" && message["role"] == "tool" {
            content.push(json!({"type": "tool_result", "tool_use_id": message["tool_call_id"], "content": message["content"]}));
        }
        Some(json!({"role": role, "content": content}))
    }).collect()
}

fn responses_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .filter_map(|tool| {
            let function = tool.get("function")?;
            Some(json!({
                "type": "function",
                "name": function["name"],
                "description": function["description"],
                "parameters": function["parameters"],
                "strict": false,
            }))
        })
        .collect()
}

fn messages_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .filter_map(|tool| {
            let function = tool.get("function")?;
            Some(json!({
                "name": function["name"],
                "description": function["description"],
                "input_schema": function["parameters"],
            }))
        })
        .collect()
}

fn system_text(messages: &[Value]) -> String {
    messages
        .iter()
        .filter(|message| message["role"] == "system")
        .filter_map(|message| message["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_responses_chunk(chunk: Value) -> Option<Value> {
    let event = chunk["type"].as_str()?;
    let id = chunk["response"]["id"]
        .as_str()
        .or_else(|| chunk["id"].as_str())
        .unwrap_or("chatcmpl-opencode");
    match event {
        "response.output_text.delta" => Some(json!({
            "id": id,
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {"content": chunk["delta"]}, "finish_reason": null}]
        })),
        "response.reasoning_summary_text.delta" => Some(json!({
            "id": id,
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {"reasoning_content": chunk["delta"]}, "finish_reason": null}]
        })),
        "response.completed" => Some(json!({
            "id": id,
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
            "usage": chunk["response"]["usage"]
        })),
        _ => None,
    }
}

fn normalize_messages_chunk(chunk: Value) -> Option<Value> {
    let event = chunk["type"].as_str()?;
    let id = chunk["message"]["id"]
        .as_str()
        .or_else(|| chunk["id"].as_str())
        .unwrap_or("chatcmpl-opencode");
    match event {
        "message_start" => Some(json!({
            "id": id,
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {"role": "assistant"}, "finish_reason": null}],
            "usage": chunk["message"]["usage"]
        })),
        "content_block_delta" if chunk["delta"]["type"] == "text_delta" => Some(json!({
            "id": id,
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {"content": chunk["delta"]["text"]}, "finish_reason": null}]
        })),
        "content_block_delta" if chunk["delta"]["type"] == "thinking_delta" => Some(json!({
            "id": id,
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {"reasoning_content": chunk["delta"]["thinking"]}, "finish_reason": null}]
        })),
        "message_delta" => Some(json!({
            "id": id,
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": chunk["delta"]["stop_reason"]}],
            "usage": chunk["usage"]
        })),
        "message_stop" => Some(json!({
            "id": id,
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::Protocol;

    #[test]
    fn selects_protocol_by_model_family() {
        assert_eq!(
            Protocol::for_model("muse-spark-1.3-contributor-free"),
            Protocol::Responses
        );
        assert_eq!(
            Protocol::for_model("muse-spark-1.3-contributor-free(high)"),
            Protocol::Responses
        );
        assert_eq!(Protocol::for_model("union-alpha"), Protocol::Messages);
        assert_eq!(Protocol::for_model("big-pickle"), Protocol::Chat);
    }

    #[test]
    fn builds_distinct_wire_shapes() {
        let request = crate::provider::opencode::ChatRequest {
            model: "muse-spark-1.3-contributor-free".into(),
            messages: vec![serde_json::json!({"role":"user","content":"hi"})],
            tools: Vec::new(),
            max_tokens: 32,
            provider: crate::config::Provider::Zen,
            protocol: Protocol::Responses,
            api_key: None,
        };
        let responses = Protocol::Responses.request_body(&request);
        assert!(responses.get("input").is_some());
        assert!(responses.get("messages").is_none());
        assert_eq!(responses["max_output_tokens"], 32);

        let messages = Protocol::Messages.request_body(&request);
        assert!(messages.get("messages").is_some());
        assert!(messages.get("system").is_some());
        assert_eq!(messages["max_tokens"], 32);
    }
}
