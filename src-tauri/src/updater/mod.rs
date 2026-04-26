use anyhow::Result;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub release_date: String,
    pub release_notes: Option<String>,
    pub download_url: Option<String>,
    pub is_mandatory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub available: bool,
    pub downloading: bool,
    pub downloaded: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

pub struct AutoUpdater {
    update_url: String,
    current_version: String,
    cache_dir: PathBuf,
}

impl AutoUpdater {
    pub fn new(update_url: &str, current_version: &str, cache_dir: PathBuf) -> Self {
        Self {
            update_url: update_url.to_string(),
            current_version: current_version.to_string(),
            cache_dir,
        }
    }

    pub async fn check_for_updates(&self) -> Result<Option<UpdateInfo>> {
        let manifest_url = format!("{}/manifest.json", self.update_url);
        
        let response = reqwest::get(&manifest_url).await?;
        
        if !response.status().is_success() {
            return Ok(None);
        }

        let manifest: serde_json::Value = response.json().await?;

        let latest_version = manifest["version"].as_str().unwrap_or("0.0.0");
        
        if self.is_newer_version(latest_version) {
            Ok(Some(UpdateInfo {
                version: latest_version.to_string(),
                release_date: manifest["release_date"].as_str().unwrap_or("").to_string(),
                release_notes: manifest["release_notes"].as_str().map(|s| s.to_string()),
                download_url: manifest["download_url"].as_str().map(|s| s.to_string()),
                is_mandatory: manifest["is_mandatory"].as_bool().unwrap_or(false),
            }))
        } else {
            Ok(None)
        }
    }

    fn is_newer_version(&self, latest: &str) -> bool {
        let current = self.current_version.trim_start_matches('v');
        let latest = latest.trim_start_matches('v');

        let current_parts: Vec<u32> = current.split('.')
            .filter_map(|s| s.parse().ok())
            .collect();
        let latest_parts: Vec<u32> = latest.split('.')
            .filter_map(|s| s.parse().ok())
            .collect();

        for (c, l) in current_parts.iter().zip(latest_parts.iter()) {
            if l > c {
                return true;
            } else if l < c {
                return false;
            }
        }

        latest_parts.len() > current_parts.len()
    }

    pub async fn download_update(&self, url: &str) -> Result<PathBuf> {
        let response = reqwest::get(url).await?;
        let total_bytes = response.content_length().unwrap_or(0);
        
        let file_name = url.split('/').last().unwrap_or("update");
        let output_path = self.cache_dir.join(file_name);

        std::fs::create_dir_all(&self.cache_dir)?;
        
        let mut file = tokio::fs::File::create(&output_path).await?;
        let mut bytes_downloaded = 0u64;

        let mut stream = response.bytes_stream();
        use tokio::io::AsyncWriteExt;
        
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            tokio::io::copy(&mut chunk.as_ref(), &mut file).await?;
            bytes_downloaded += chunk.len() as u64;
        }

        Ok(output_path)
    }

    pub fn verify_signature(&self, _file_path: &PathBuf, _signature: &str) -> Result<bool> {
        Ok(true)
    }

    pub fn install_update(&self, file_path: &PathBuf) -> Result<()> {
        #[cfg(windows)]
        {
            use std::process::Command;
            Command::new("cmd.exe")
                .args(["/C", "start", "", file_path.to_str().unwrap_or("")])
                .spawn()?;
        }

        #[cfg(not(windows))]
        {
            use std::process::Command;
            Command::new("open")
                .arg(file_path)
                .spawn()?;
        }

        Ok(())
    }
}