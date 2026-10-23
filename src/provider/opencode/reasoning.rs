use serde_json::Value;

const PLACEHOLDER: &str = " ";

/// Preserve the reasoning field required by OpenCode Zen on assistant history.
pub fn with_reasoning_content(messages: &[Value]) -> Vec<Value> {
    messages
        .iter()
        .map(|message| {
            let is_assistant = message.get("role").and_then(Value::as_str) == Some("assistant");
            let has_reasoning = message
                .get("reasoning_content")
                .and_then(Value::as_str)
                .is_some_and(|content| !content.is_empty());
            if !is_assistant || has_reasoning {
                return message.clone();
            }
            let mut message = message.clone();
            if let Some(object) = message.as_object_mut() {
                object.insert(
                    "reasoning_content".into(),
                    Value::String(PLACEHOLDER.into()),
                );
            }
            message
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::with_reasoning_content;
    use serde_json::json;

    #[test]
    fn fills_missing_reasoning_without_overwriting_existing_content() {
        let messages = vec![
            json!({"role":"assistant","content":"old"}),
            json!({"role":"assistant","content":"kept","reasoning_content":"real"}),
            json!({"role":"user","content":"next"}),
        ];
        let messages = with_reasoning_content(&messages);
        assert_eq!(messages[0]["reasoning_content"], " ");
        assert_eq!(messages[1]["reasoning_content"], "real");
        assert!(messages[2].get("reasoning_content").is_none());
    }
}
