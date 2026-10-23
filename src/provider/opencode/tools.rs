use serde_json::{json, Value};

fn required_tool(name: &str) -> Value {
    match name {
        "read" => json!({
            "type": "function",
            "function": {
                "name": "read",
                "description": "Read the contents of a file or directory. Supports text files, images, and PDFs. Images and PDFs are presented directly to the model. Each text line is prefixed by its 1-based line number as <line>: <content>. The prefix is for reference and is not part of the file content. Directory entries are returned one per line. Use offset and limit to read large files or directories in sections. Prefer one larger read over many small slices, and use grep to find specific content in large files.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "File or directory to read"},
                        "offset": {"type": "integer", "minimum": 0, "description": "The line or directory entry to start reading from (1-based)"},
                        "limit": {"type": "integer", "minimum": 0, "description": "The maximum number of lines or directory entries to read (defaults to and capped at 2000)"}
                    },
                    "required": ["path"],
                    "additionalProperties": false
                }
            }
        }),
        "shell" => json!({
            "type": "function",
            "function": {
                "name": "shell",
                "description": "Execute a shell command and return its output. Commands run on Linux using bash. Quote file paths containing spaces or special characters. Prefer dedicated tools over shell commands when possible. When output is large, the full result is saved to a file and a truncated preview is returned. Rely on automatic truncation unless filtering the output is more useful. Commands accept an optional timeout, background commands have no timeout by default. Background commands return immediately, and you will be notified when they complete.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "Shell command string to execute"},
                        "workdir": {"type": "string", "description": "Working directory to execute the command in. Defaults to the current working directory. When possible, avoid changing directories in the command and set the working directory here instead."},
                        "timeout": {"type": "integer", "minimum": 0, "description": "Timeout in milliseconds. Set to 0 to disable the timeout. Defaults to 120000 for foreground commands. Background commands have no timeout by default."},
                        "background": {"type": "boolean", "description": "Run the command in the background and return immediately (useful for dev servers and long-running builds). You do not need to use '&' at the end of the command when using this parameter. You will be notified when they complete. DO NOT poll for completion."}
                    },
                    "required": ["command"],
                    "additionalProperties": false
                }
            }
        }),
        _ => unreachable!(),
    }
}

fn tool_name(tool: &Value) -> Option<&str> {
    tool["function"]["name"].as_str()
}

fn is_exact_fingerprint_tool(tool: &Value, name: &str) -> bool {
    let canonical = required_tool(name);
    tool["function"]["description"] == canonical["function"]["description"]
        && tool["function"]["parameters"] == canonical["function"]["parameters"]
}

fn with_tool_name(mut tool: Value, name: String) -> Value {
    tool["function"]["name"] = Value::String(name);
    tool
}

/// Add the exact OpenCode v2 fingerprint tools required by free inference.
pub fn with_required_tools(tools: Vec<Value>) -> Vec<Value> {
    let fingerprint_names = ["read", "shell"];
    let mut taken: std::collections::HashSet<String> = tools
        .iter()
        .filter_map(|tool| tool_name(tool).map(str::to_string))
        .collect();
    let mut canonical_seen = std::collections::HashSet::new();
    let mut output = Vec::with_capacity(tools.len() + fingerprint_names.len());

    for tool in tools {
        let name = tool_name(&tool).map(str::to_string);
        let Some(name) = name else {
            output.push(tool);
            continue;
        };
        if !fingerprint_names.contains(&name.as_str()) {
            output.push(tool);
            continue;
        }
        if is_exact_fingerprint_tool(&tool, &name) && canonical_seen.insert(name.clone()) {
            output.push(tool);
            continue;
        }

        let mut alias = format!("{name}_client");
        let mut suffix = 2;
        while taken.contains(&alias) {
            alias = format!("{name}_client{suffix}");
            suffix += 1;
        }
        taken.insert(alias.clone());
        output.push(with_tool_name(tool, alias));
    }

    for name in fingerprint_names {
        if !canonical_seen.contains(name) {
            output.push(required_tool(name));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::with_required_tools;
    use serde_json::json;

    #[test]
    fn injects_exact_read_and_shell_tools() {
        let tools = with_required_tools(Vec::new());
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["function"]["name"], "read");
        assert_eq!(tools[1]["function"]["name"], "shell");
        assert_eq!(
            tools[0]["function"]["parameters"]["required"],
            json!(["path"])
        );
        assert_eq!(
            tools[1]["function"]["parameters"]["required"],
            json!(["command"])
        );
        assert!(tools[0]["function"]["description"]
            .as_str()
            .unwrap()
            .starts_with("Read the contents"));
        assert!(tools[1]["function"]["description"]
            .as_str()
            .unwrap()
            .starts_with("Execute a shell"));
    }

    #[test]
    fn aliases_conflicting_client_fingerprint_names() {
        let tools = with_required_tools(vec![json!({
            "type": "function",
            "function": {
                "name": "read",
                "description": "client read",
                "parameters": {"type": "object"}
            }
        })]);
        let names: Vec<_> = tools
            .iter()
            .filter_map(|tool| tool["function"]["name"].as_str())
            .collect();
        assert_eq!(names, ["read_client", "read", "shell"]);
        assert_eq!(tools[0]["function"]["description"], "client read");
    }
}
