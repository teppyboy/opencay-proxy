use super::protocol::Protocol;
use crate::config::{Provider, RuntimeConfig};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub raw: Value,
    pub provider: Provider,
    pub protocol: Protocol,
    pub free: bool,
}

#[derive(Debug, serde::Deserialize)]
struct ModelsResponse {
    data: Vec<Value>,
}

pub async fn fetch(config: &RuntimeConfig) -> Result<HashMap<String, ModelInfo>, reqwest::Error> {
    let mut catalog = HashMap::new();
    let client = reqwest::Client::new();
    add_models(
        &client,
        "https://opencode.ai/inference/v1/models",
        Provider::Zen,
        config.has_keys(Provider::Zen),
        &mut catalog,
    )
    .await?;
    if config.has_keys(Provider::Go) {
        add_models(
            &client,
            "https://opencode.ai/inference/go/v1/models",
            Provider::Go,
            true,
            &mut catalog,
        )
        .await?;
    }
    Ok(catalog)
}

async fn add_models(
    client: &reqwest::Client,
    endpoint: &str,
    provider: Provider,
    include_paid: bool,
    catalog: &mut HashMap<String, ModelInfo>,
) -> Result<(), reqwest::Error> {
    let response = client
        .get(endpoint)
        .send()
        .await?
        .error_for_status()?
        .json::<ModelsResponse>()
        .await?;
    for raw in response.data {
        let Some(id) = raw.get("id").and_then(Value::as_str).map(str::to_owned) else {
            continue;
        };
        let free = id.ends_with("-free") || id == "big-pickle";
        if free || include_paid {
            catalog.insert(
                id.clone(),
                ModelInfo {
                    id: id.clone(),
                    raw,
                    provider,
                    protocol: protocol_for(provider, &id),
                    free,
                },
            );
        }
    }
    Ok(())
}

fn protocol_for(provider: Provider, model: &str) -> Protocol {
    let base = model.split('(').next().unwrap_or(model);
    match (provider, base) {
        (Provider::Zen, "union-alpha") => Protocol::Messages,
        (_, value) if value.starts_with("muse-spark") => Protocol::Responses,
        (_, value) if value.starts_with("grok-") || value.starts_with("gpt-") => {
            Protocol::Responses
        }
        (_, value)
            if value.starts_with("claude-")
                || value.starts_with("qwen3.7")
                || value.starts_with("qwen3.8") =>
        {
            Protocol::Messages
        }
        _ => Protocol::Chat,
    }
}

#[cfg(test)]
mod tests {
    use super::protocol_for;
    use crate::config::Provider;
    use crate::provider::opencode::Protocol;

    #[test]
    fn selects_current_inference_protocol_families() {
        assert_eq!(
            protocol_for(Provider::Zen, "muse-spark-1.3-contributor"),
            Protocol::Responses
        );
        assert_eq!(
            protocol_for(Provider::Go, "gpt-6-luna"),
            Protocol::Responses
        );
        assert_eq!(
            protocol_for(Provider::Go, "claude-haiku-5-5"),
            Protocol::Messages
        );
        assert_eq!(protocol_for(Provider::Go, "glm-5.3"), Protocol::Chat);
    }
}
