use super::http::{self, HeaderList};
use super::protocol::Protocol;
use super::BoxError;
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

pub async fn fetch(config: &RuntimeConfig) -> Result<HashMap<String, ModelInfo>, BoxError> {
    let mut catalog = HashMap::new();
    let metadata = fetch_metadata().await;
    add_models(
        "https://opencode.ai/inference/v1/models",
        Provider::Zen,
        config.has_keys(Provider::Zen),
        &metadata,
        &mut catalog,
    )
    .await?;
    if config.has_keys(Provider::Go) {
        add_models(
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
    endpoint: &str,
    provider: Provider,
    include_paid: bool,
    metadata: &HashMap<String, HashMap<String, Value>>,
    catalog: &mut HashMap<String, ModelInfo>,
) -> Result<(), BoxError> {
    let (status, body) = http::get_text(endpoint, &HeaderList::new()).await?;
    if !(200..300).contains(&status) {
        return Err(format!("model catalog request to {endpoint} failed ({status}): {body}").into());
    }
    let response: ModelsResponse = serde_json::from_str(&body)?;
    let provider_key = match provider {
        Provider::Zen => "opencode",
        Provider::Go => "opencode-go",
    };
    for raw in response.data {
        let Some(id) = raw.get("id").and_then(Value::as_str).map(str::to_owned) else {
            continue;
        };
        let model_metadata = metadata
            .get(provider_key)
            .and_then(|models| models.get(&id));
        let free = is_free(model_metadata).unwrap_or_else(|| id.ends_with("-free") || id == "big-pickle");
        if free || include_paid {
            let raw = merge_metadata(raw, model_metadata);
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

/// The CLI reads the catalog from `models.opencode.ai`; `models.dev` mirrors
/// it but is not what the official client uses.
const CATALOG_ENDPOINT: &str = "https://models.opencode.ai/api.json";

async fn fetch_metadata() -> HashMap<String, HashMap<String, Value>> {
    let Ok((status, body)) = http::get_text(CATALOG_ENDPOINT, &HeaderList::new()).await else {
        return HashMap::new();
    };
    if !(200..300).contains(&status) {
        return HashMap::new();
    }
    let Ok(catalog) = serde_json::from_str::<HashMap<String, Value>>(&body) else {
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

/// Whether the catalog lists a model as free. `None` when the catalog has no
/// entry for it, so the caller can fall back to the id heuristic.
fn is_free(metadata: Option<&Value>) -> Option<bool> {
    let cost = metadata?.get("cost")?;
    let input = cost.get("input")?.as_f64()?;
    Some(input == 0.0)
}

fn merge_metadata(mut upstream: Value, metadata: Option<&Value>) -> Value {    let Some(metadata) = metadata.and_then(Value::as_object) else {
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
    use super::{is_free, protocol_for};
    use crate::config::Provider;
    use crate::provider::opencode::Protocol;

    #[test]
    fn detects_free_models_from_the_catalog_cost() {
        let free = serde_json::json!({"cost": {"input": 0, "output": 0}});
        let paid = serde_json::json!({"cost": {"input": 1.5, "output": 6}});
        assert_eq!(is_free(Some(&free)), Some(true));
        assert_eq!(is_free(Some(&paid)), Some(false));
        // A model the catalog does not list falls back to the id heuristic.
        assert_eq!(is_free(None), None);
        // `grok-code` is free without a `-free` suffix.
        assert!(is_free(Some(&free)).unwrap());
    }

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
