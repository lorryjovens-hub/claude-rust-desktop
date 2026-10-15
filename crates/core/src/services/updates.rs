use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    #[serde(default)]
    pub release_date: String,
    pub release_notes: Option<String>,
    pub download_url: Option<String>,
    #[serde(default)]
    pub is_mandatory: bool,
    pub signature: Option<String>,
}
pub async fn check(manifest_url: &str, current: &str) -> Result<Option<UpdateInfo>> {
    super::validate_endpoint(manifest_url)?;
    let current = semver::Version::parse(current.trim_start_matches('v'))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client.get(manifest_url).send().await?.error_for_status()?;
    ensure!(
        response.status().is_success(),
        "Update manifest returned unexpected status {}",
        response.status()
    );
    ensure!(
        response.content_length().unwrap_or(0) <= 1_048_576,
        "Update manifest too large"
    );
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= 1_048_576,
            "Update manifest too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    let info: UpdateInfo = serde_json::from_slice(&bytes)?;
    let latest = semver::Version::parse(info.version.trim_start_matches('v'))?;
    Ok((latest > current).then_some(info))
}
