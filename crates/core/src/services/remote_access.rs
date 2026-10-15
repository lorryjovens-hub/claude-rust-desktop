//! Explicit remote operations. Hosts authorize these calls; no model credentials are persisted.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    pub host: String,
    pub username: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub private_key_path: Option<PathBuf>,
    pub known_hosts_path: Option<PathBuf>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}
fn default_port() -> u16 {
    22
}
fn default_timeout() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SshOperation {
    Check,
    Execute { command: String },
    Upload { local: PathBuf, remote: String },
    Download { remote: String, local: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

impl SshConfig {
    fn args(&self, scp: bool) -> Result<Vec<String>> {
        if self.host.is_empty()
            || self.host.starts_with('-')
            || !self
                .host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b))
        {
            bail!("Invalid SSH host");
        }
        if self.username.is_empty()
            || !self
                .username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            bail!("Invalid SSH username");
        }
        if self.port == 0 || !(1..=3600).contains(&self.timeout_secs) {
            bail!("Invalid SSH port or timeout");
        }
        let mut args = vec![
            if scp { "-P" } else { "-p" }.into(),
            self.port.to_string(),
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "StrictHostKeyChecking=yes".into(),
            "-o".into(),
            format!("ConnectTimeout={}", self.timeout_secs),
            "-o".into(),
            "ServerAliveInterval=15".into(),
            "-o".into(),
            "ServerAliveCountMax=2".into(),
        ];
        if let Some(path) = &self.private_key_path {
            args.extend(["-i".into(), path.to_string_lossy().into_owned()]);
        }
        if let Some(path) = &self.known_hosts_path {
            args.extend([
                "-o".into(),
                format!("UserKnownHostsFile={}", path.display()),
            ]);
        }
        Ok(args)
    }
}

/// Uses OpenSSH key/agent authentication and strict host verification. Check runs a real
/// remote command; no synthetic connected status is stored. Cancellation drops the child.
pub async fn ssh(config: &SshConfig, operation: SshOperation) -> Result<CommandResult> {
    let scp = matches!(
        operation,
        SshOperation::Upload { .. } | SshOperation::Download { .. }
    );
    let mut args = config.args(scp)?;
    let mut download = None;
    let target = format!("{}@{}", config.username, config.host);
    match operation {
        SshOperation::Check => args.extend([target, "exit 0".into()]),
        SshOperation::Execute { command } => {
            if command.trim().is_empty() {
                bail!("SSH command is empty");
            }
            args.extend([target, command]);
        }
        SshOperation::Upload { local, remote } => {
            validate_remote_path(&remote)?;
            let local = local
                .canonicalize()
                .context("Upload source does not exist")?;
            if !local.is_file() {
                bail!("Upload source must be a file");
            }
            args.extend([
                local.to_string_lossy().into_owned(),
                format!("{target}:{remote}"),
            ]);
        }
        SshOperation::Download { remote, local } => {
            validate_remote_path(&remote)?;
            // OpenSSH SCP may overwrite; require a new destination explicitly.
            if local.exists() {
                bail!("Download destination already exists");
            }
            let parent = local
                .parent()
                .context("Download destination needs a parent directory")?
                .canonicalize()?;
            let name = local
                .file_name()
                .context("Download destination needs a file name")?;
            let local = parent.join(name);
            let temporary = tempfile::NamedTempFile::new_in(&parent)?;
            args.extend([
                format!("{target}:{remote}"),
                temporary.path().to_string_lossy().into_owned(),
            ]);
            download = Some((temporary, local));
        }
    }
    let started = Instant::now();
    let mut command = tokio::process::Command::new(if scp { "scp" } else { "ssh" });
    command
        .args(args)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(config.timeout_secs), command.output())
        .await
        .context("SSH operation timed out")?
        .context("OpenSSH executable is unavailable")?;
    if output.status.success() {
        if let Some((temporary, destination)) = download {
            temporary
                .persist_noclobber(destination)
                .context("Publish SSH download without overwriting")?;
        }
    }
    Ok(CommandResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn validate_remote_path(path: &str) -> Result<()> {
    if !path.starts_with('/')
        || !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
    {
        bail!("Remote file path must be absolute and contain only letters, digits, slash, dot, underscore or hyphen");
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRequest {
    pub endpoint: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}
fn default_method() -> String {
    "GET".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteResult {
    pub status_code: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
    pub duration_ms: u64,
}

/// No shared cache or automatic retries: credentials cannot cross cache entries and
/// non-idempotent requests cannot be replayed after ambiguous transport failures.
pub async fn request(input: RemoteRequest) -> Result<RemoteResult> {
    let url = reqwest::Url::parse(&input.endpoint)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("Remote endpoint must be HTTP(S), with credentials in headers");
    }
    if !(1..=3600).contains(&input.timeout_secs) {
        bail!("Timeout must be between 1 and 3600 seconds");
    }
    let method = reqwest::Method::from_bytes(input.method.to_ascii_uppercase().as_bytes())?;
    if ![
        reqwest::Method::GET,
        reqwest::Method::POST,
        reqwest::Method::PUT,
        reqwest::Method::PATCH,
        reqwest::Method::DELETE,
        reqwest::Method::HEAD,
    ]
    .contains(&method)
    {
        bail!("Unsupported HTTP method");
    }
    let started = Instant::now();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(input.timeout_secs))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut request = client.request(method, url);
    for (key, value) in input.headers {
        request = request.header(key, value);
    }
    if let Some(body) = input.body {
        request = request.body(body);
    }
    let mut response = request.send().await?;
    let status_code = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_str().unwrap_or("").to_owned()))
        .collect();
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > 8 * 1024 * 1024 {
            bail!("Remote response exceeds 8 MiB");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(RemoteResult {
        status_code,
        headers,
        body: String::from_utf8(body).context("Remote response is not UTF-8 text")?,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ssh_validates_options_and_preserves_host_verification() {
        let mut config = SshConfig {
            host: "localhost".into(),
            username: "tester".into(),
            port: 22,
            private_key_path: Some("key with spaces".into()),
            known_hosts_path: None,
            timeout_secs: 10,
        };
        let args = config.args(false).unwrap();
        assert!(args.contains(&"StrictHostKeyChecking=yes".into()));
        assert!(args.contains(&"BatchMode=yes".into()));
        assert!(args.contains(&"key with spaces".into()));
        config.host = "-oProxyCommand=bad".into();
        assert!(config.args(false).is_err());
        assert!(validate_remote_path("/safe/file.txt").is_ok());
        assert!(validate_remote_path("/file;touch bad").is_err());
    }
    #[tokio::test]
    async fn http_returns_actual_status_and_body_without_replaying_posts() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0u8; 4096];
            let length = socket.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..length]);
            assert!(request.starts_with("POST /"));
            assert!(request.contains("x-test: fixture"));
            socket.write_all(b"HTTP/1.1 409 Conflict\r\nContent-Length: 8\r\nConnection: close\r\n\r\nconflict").await.unwrap();
        });
        let result = request(RemoteRequest {
            endpoint,
            method: "POST".into(),
            headers: [("x-test".into(), "fixture".into())].into(),
            body: Some("payload".into()),
            timeout_secs: 2,
        })
        .await
        .unwrap();
        assert_eq!(result.status_code, 409);
        assert_eq!(result.body, "conflict");
        server.await.unwrap();
    }
}
