use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::OnceCell;

const LATEST_PACKAGE_ENDPOINT: &str = "https://registry.npmjs.org/opencode-ai/latest";
const FALLBACK_VERSION: &str = "2.0.26";

static VERSION: OnceCell<String> = OnceCell::const_new();

#[derive(Debug, Deserialize)]
struct PackageMetadata {
    version: String,
}

/// Return whether a version has the v2 OpenCode client shape required by free inference.
pub fn is_supported(version: &str) -> bool {
    let mut parts = version.split('.');
    let Some(major) = parts.next().and_then(|part| part.parse::<u32>().ok()) else {
        return false;
    };
    major >= 2
        && parts.next().is_some_and(|part| part.parse::<u32>().is_ok())
        && parts.next().is_some_and(|part| part.parse::<u32>().is_ok())
        && parts.next().is_none()
}

/// Fetch the current OpenCode CLI version once, falling back to a known v2 shape
/// when the registry is unavailable. The request is not repeated per completion.
pub async fn user_agent() -> String {
    let version = VERSION
        .get_or_init(|| async {
            fetch_latest()
                .await
                .filter(|version| is_supported(version))
                .unwrap_or_else(|| FALLBACK_VERSION.to_string())
        })
        .await;
    format!("opencode/latest/{version}/cli")
}

async fn fetch_latest() -> Option<String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok()?;
    client
        .get(LATEST_PACKAGE_ENDPOINT)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<PackageMetadata>()
        .await
        .ok()
        .map(|metadata| metadata.version)
}

#[cfg(test)]
mod tests {
    use super::is_supported;

    #[test]
    fn accepts_only_complete_v2_versions() {
        assert!(is_supported("2.0.26"));
        assert!(is_supported("20.1.3"));
        assert!(!is_supported("1.19.3"));
        assert!(!is_supported("2.0"));
        assert!(!is_supported("2.0.0-beta.1"));
        assert!(!is_supported("latest"));
    }
}
