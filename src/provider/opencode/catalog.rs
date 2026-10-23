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
    let metadata = fetch_metadata(&client).await;
    add_models(
        &client,
        "https://opencode.ai/inference/v1/models",
        Provider::Zen,
        config.has_keys(Provider::Zen),
        &metadata,
        &mut catalog,
    )
    .await?;
    if config.has_keys(Provider::Go) {
        add_models(
            &client,
            "https://opencode.ai/inference/go/v1/models",
            Provider::Go,
            true,
            &metadata,
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
    metadata: &HashMap<String, HashMap<String, Value>>,
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
            let provider_key = match provider {
                Provider::Zen => "opencode",
                Provider::Go => "opencode-go",
            };
            let raw = merge_metadata(
                raw,
                metadata
                    .get(provider_key)
                    .and_then(|models| models.get(&id)),
            );
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

async fn fetch_metadata(client: &reqwest::Client) -> HashMap<String, HashMap<String, Value>> {
    let Ok(response) = client.get("https://models.dev/api.json").send().await else {
        return HashMap::new();
    };
    let Ok(catalog) = response.json::<HashMap<String, Value>>().await else {
        return HashMap::new();
    };
    ["opencode", "opencode-go"]
        .into_iter()
        .filter_map(|provider| {
            let models = catalog
                .get(provider)?
                .get("models")?
                .as_object()?
                .iter()
                .map(|(id, metadata)| (id.clone(), metadata.clone()))
                .collect();
            Some((provider.to_string(), models))
        })
        .collect()
}

fn merge_metadata(mut upstream: Value, metadata: Option<&Value>) -> Value {
    let Some(metadata) = metadata.and_then(Value::as_object) else {
        return upstream;
    };
    let Some(upstream_object) = upstream.as_object_mut() else {
        return upstream;
    };
    for (key, value) in metadata {
        upstream_object
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
    upstream
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
    fn preserves_upstream_fields_and_adds_missing_catalog_metadata() {
        let upstream = serde_json::json!({"id": "glm-5.3", "created": 1, "owned_by": "opencode"});
        let metadata = serde_json::json!({"name": "GLM-5.3", "limit": {"context": 1_000_000}});
        let merged = super::merge_metadata(upstream, Some(&metadata));
        assert_eq!(merged["id"], "glm-5.3");
        assert_eq!(merged["created"], 1);
        assert_eq!(merged["name"], "GLM-5.3");
        assert_eq!(merged["limit"]["context"], 1_000_000);
    }

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
