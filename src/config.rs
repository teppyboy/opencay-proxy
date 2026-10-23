use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub opencode: OpenCodeConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OpenCodeConfig {
    #[serde(default)]
    pub zen: EndpointConfig,
    #[serde(default)]
    pub go: EndpointConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EndpointConfig {
    #[serde(default)]
    pub api_keys: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Zen,
    Go,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub zen_keys: Arc<Mutex<KeyPool>>,
    pub go_keys: Arc<Mutex<KeyPool>>,
}

#[derive(Debug, Clone)]
pub struct KeyPool {
    keys: Vec<String>,
    next: usize,
}

impl KeyPool {
    pub fn new(keys: Vec<String>) -> Self {
        Self {
            keys: keys
                .into_iter()
                .filter(|key| !key.trim().is_empty())
                .collect(),
            next: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn next(&mut self) -> Option<String> {
        if self.keys.is_empty() {
            return None;
        }
        let key = self.keys[self.next % self.keys.len()].clone();
        self.next = self.next.wrapping_add(1);
        Some(key)
    }
}

impl RuntimeConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        let file: ConfigFile = if contents.is_empty() {
            ConfigFile::default()
        } else {
            toml::from_str(&contents)?
        };
        Ok(Self {
            zen_keys: Arc::new(Mutex::new(KeyPool::new(file.opencode.zen.api_keys))),
            go_keys: Arc::new(Mutex::new(KeyPool::new(file.opencode.go.api_keys))),
        })
    }

    pub fn has_keys(&self, provider: Provider) -> bool {
        !self
            .pool(provider)
            .lock()
            .expect("key pool poisoned")
            .is_empty()
    }

    pub fn next_key(&self, provider: Provider) -> Option<String> {
        self.pool(provider)
            .lock()
            .expect("key pool poisoned")
            .next()
    }

    fn pool(&self, provider: Provider) -> &Arc<Mutex<KeyPool>> {
        match provider {
            Provider::Zen => &self.zen_keys,
            Provider::Go => &self.go_keys,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyPool, Provider, RuntimeConfig};
    use std::fs;

    #[test]
    fn rotates_keys_without_fallback_behavior() {
        let mut pool = KeyPool::new(vec!["a".into(), "b".into()]);
        assert_eq!(pool.next().as_deref(), Some("a"));
        assert_eq!(pool.next().as_deref(), Some("b"));
        assert_eq!(pool.next().as_deref(), Some("a"));
    }

    #[test]
    fn loads_separate_zen_and_go_key_pools() {
        let path = std::env::temp_dir().join(format!("opencay-config-{}", std::process::id()));
        fs::write(
            &path,
            "[opencode.zen]\napi_keys=[\"z\"]\n[opencode.go]\napi_keys=[\"g1\",\"g2\"]\n",
        )
        .unwrap();
        let config = RuntimeConfig::load(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert!(config.has_keys(Provider::Zen));
        assert!(config.has_keys(Provider::Go));
        assert_eq!(config.next_key(Provider::Zen).as_deref(), Some("z"));
        assert_eq!(config.next_key(Provider::Go).as_deref(), Some("g1"));
    }
}
