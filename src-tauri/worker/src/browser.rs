use anyhow::{anyhow, Result};
use reqwest::Client;

pub struct BrowserEngine {
    client: Client,
}

impl BrowserEngine {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .danger_accept_invalid_certs(true)
                .user_agent("Agent-Worker/0.1")
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn open_url(&self, url: &str) -> Result<String> {
        let resp = self
            .client
            .get(url)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch URL '{}': {}", url, e))?;

        if !resp.status().is_success() {
            return Err(anyhow!(
                "HTTP {} when fetching '{}'",
                resp.status(),
                url
            ));
        }

        let body = resp.text().await?;
        tracing::info!("BrowserEngine: opened {} ({} bytes)", url, body.len());
        Ok(body)
    }

    pub async fn open_browser_app(&self, url: &str) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", url])
                .spawn()
                .map_err(|e| anyhow!("Failed to launch browser: {}", e))?;
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(url)
                .spawn()
                .map_err(|e| anyhow!("Failed to launch browser: {}", e))?;
        }

        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(url)
                .spawn()
                .map_err(|e| anyhow!("Failed to launch browser: {}", e))?;
        }

        Ok(())
    }
}