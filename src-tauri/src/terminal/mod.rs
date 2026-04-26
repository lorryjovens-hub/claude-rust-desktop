use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtySession {
    pub id: String,
    pub cwd: String,
    pub pid: u32,
    pub shell: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyOutput {
    pub session_id: String,
    pub data: String,
    pub is_stderr: bool,
}

pub struct PtyManager {
    sessions: Arc<Mutex<HashMap<String, PtySession>>>,
    outputs: Arc<Mutex<HashMap<String, broadcast::Sender<PtyOutput>>>>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            outputs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn create_session(&self, cwd: Option<String>, shell: Option<String>) -> Result<PtySession> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let cwd = cwd.unwrap_or_else(|| std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string()));

        let shell = if cfg!(windows) {
            shell.unwrap_or_else(|| "cmd.exe".to_string())
        } else {
            shell.unwrap_or_else(|| "/bin/bash".to_string())
        };

        let session = PtySession {
            id: session_id.clone(),
            cwd: cwd.clone(),
            pid: 0,
            shell: shell.clone(),
        };

        let (tx, _rx) = broadcast::channel(100);
        
        self.sessions.lock().await.insert(session_id.clone(), session.clone());
        self.outputs.lock().await.insert(session_id.clone(), tx);

        Ok(session)
    }

    pub async fn write_input(&self, session_id: &str, data: &str) -> Result<()> {
        let outputs = self.outputs.lock().await;
        if let Some(tx) = outputs.get(session_id) {
            tx.send(PtyOutput {
                session_id: session_id.to_string(),
                data: data.to_string(),
                is_stderr: false,
            })?;
        }
        Ok(())
    }

    pub async fn resize(&self, session_id: &str, _cols: u16, _rows: u16) -> Result<()> {
        let sessions = self.sessions.lock().await;
        if !sessions.contains_key(session_id) {
            return Err(anyhow::anyhow!("Session not found"));
        }
        Ok(())
    }

    pub async fn close_session(&self, session_id: &str) -> Result<()> {
        self.sessions.lock().await.remove(session_id);
        self.outputs.lock().await.remove(session_id);
        Ok(())
    }

    pub async fn list_sessions(&self) -> Vec<PtySession> {
        self.sessions.lock().await.values().cloned().collect()
    }

    pub async fn subscribe(&self, session_id: &str) -> Option<broadcast::Receiver<PtyOutput>> {
        self.outputs.lock().await.get(session_id).map(|tx| tx.subscribe())
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn execute_bash_command(
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
    env_vars: Option<HashMap<String, String>>,
) -> Result<String> {
    let cwd = cwd.unwrap_or(".");
    let is_win = cfg!(windows);

    let mut cmd = if is_win {
        let mut c = Command::new("cmd.exe");
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("bash");
        c.args(["-c", command]);
        c
    };

    cmd.current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(env) = env_vars {
        let envs: Vec<(String, String)> = env.into_iter().chain([
            ("LANG".to_string(), "en_US.UTF-8".to_string()),
            ("TERM".to_string(), "xterm-256color".to_string()),
        ]).collect();
        cmd.envs(envs);
    } else {
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("TERM", "xterm-256color");
    }

    let timeout = tokio::time::Duration::from_secs(timeout_secs);
    
    let result = tokio::time::timeout(timeout, cmd.output()).await;
    
    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            
            let mut result = stdout.to_string();
            if !stderr.is_empty() {
                result.push_str(&format!("\nSTDERR: {}", stderr));
            }
            
            if !output.status.success() && stdout.is_empty() {
                result = format!("Command exited with code {:?}: {}", output.status.code(), stderr);
            }
            
            Ok(result)
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("Failed to execute command: {}", e)),
        Err(_) => Err(anyhow::anyhow!("Command timed out after {} seconds", timeout_secs)),
    }
}