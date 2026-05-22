use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub max_execution_time: u64,
    pub max_memory_mb: u64,
    pub allowed_languages: Vec<String>,
    pub max_output_size: usize,
    pub allow_network: bool,
    pub allow_filesystem: bool,
    pub allowed_paths: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_execution_time: 30,
            max_memory_mb: 512,
            allowed_languages: vec![
                "python".to_string(),
                "javascript".to_string(),
                "rust".to_string(),
                "go".to_string(),
                "bash".to_string(),
                "powershell".to_string(),
            ],
            max_output_size: 10000,
            allow_network: false,
            allow_filesystem: true,
            allowed_paths: vec!["/tmp".to_string(), "/sandbox".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub language: String,
    pub code: String,
    pub input: Option<String>,
    pub timeout: Option<u64>,
    pub environment: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub id: String,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub execution_time_ms: u128,
    pub error: Option<String>,
}

pub struct CodeSandbox {
    config: SandboxConfig,
    execution_history: Arc<RwLock<Vec<ExecutionResult>>>,
    temp_dir: PathBuf,
}

impl CodeSandbox {
    pub fn new(temp_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&temp_dir).ok();
        Self {
            config: SandboxConfig::default(),
            execution_history: Arc::new(RwLock::new(Vec::new())),
            temp_dir,
        }
    }

    pub async fn execute(&self, req: ExecutionRequest) -> ExecutionResult {
        let id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        if !self.config.allowed_languages.contains(&req.language) {
            return ExecutionResult {
                id,
                success: false,
                stdout: String::new(),
                stderr: format!("Language '{}' is not allowed", req.language),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(format!("Unsupported language: {}", req.language)),
            };
        }

        let timeout = req.timeout.unwrap_or(self.config.max_execution_time);
        
        let result = match req.language.as_str() {
            "python" => self.execute_python(&req.code, timeout, &req.environment).await,
            "javascript" | "node" => self.execute_javascript(&req.code, timeout, &req.environment).await,
            "bash" => self.execute_bash(&req.code, timeout, &req.environment).await,
            "powershell" => self.execute_powershell(&req.code, timeout, &req.environment).await,
            "rust" => self.execute_rust(&req.code, timeout, &req.environment).await,
            "go" => self.execute_go(&req.code, timeout, &req.environment).await,
            _ => ExecutionResult {
                id: id.clone(),
                success: false,
                stdout: String::new(),
                stderr: String::new(),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(format!("Unsupported language: {}", req.language)),
            },
        };

        let mut history = self.execution_history.write().await;
        history.push(result.clone());
        
        result
    }

    async fn execute_python(
        &self,
        code: &str,
        _timeout: u64,
        env: &Option<HashMap<String, String>>,
    ) -> ExecutionResult {
        let id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        let script_path = self.temp_dir.join(format!("{}.py", id));
        if let Err(e) = std::fs::write(&script_path, code) {
            return ExecutionResult {
                id,
                success: false,
                stdout: String::new(),
                stderr: format!("Failed to write script: {}", e),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(e.to_string()),
            };
        }

        let mut cmd = Command::new("python");
        cmd.arg(&script_path);
        
        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let output = cmd.output();
        
        std::fs::remove_file(&script_path).ok();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout)
                    .chars()
                    .take(self.config.max_output_size)
                    .collect();
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(self.config.max_output_size)
                    .collect();

                ExecutionResult {
                    id,
                    success: output.status.success(),
                    stdout,
                    stderr,
                    exit_code: output.status.code().unwrap_or(-1),
                    execution_time_ms: start_time.elapsed().as_millis(),
                    error: None,
                }
            }
            Err(e) => ExecutionResult {
                id,
                success: false,
                stdout: String::new(),
                stderr: String::new(),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(e.to_string()),
            },
        }
    }

    async fn execute_javascript(
        &self,
        code: &str,
        _timeout: u64,
        env: &Option<HashMap<String, String>>,
    ) -> ExecutionResult {
        let id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        let script_path = self.temp_dir.join(format!("{}.js", id));
        if let Err(e) = std::fs::write(&script_path, code) {
            return ExecutionResult {
                id,
                success: false,
                stdout: String::new(),
                stderr: format!("Failed to write script: {}", e),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(e.to_string()),
            };
        }

        let mut cmd = Command::new("node");
        cmd.arg(&script_path);
        
        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let output = cmd.output();
        
        std::fs::remove_file(&script_path).ok();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout)
                    .chars()
                    .take(self.config.max_output_size)
                    .collect();
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(self.config.max_output_size)
                    .collect();

                ExecutionResult {
                    id,
                    success: output.status.success(),
                    stdout,
                    stderr,
                    exit_code: output.status.code().unwrap_or(-1),
                    execution_time_ms: start_time.elapsed().as_millis(),
                    error: None,
                }
            }
            Err(e) => ExecutionResult {
                id,
                success: false,
                stdout: String::new(),
                stderr: String::new(),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(e.to_string()),
            },
        }
    }

    async fn execute_bash(
        &self,
        code: &str,
        _timeout: u64,
        env: &Option<HashMap<String, String>>,
    ) -> ExecutionResult {
        let id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(code);
        
        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let output = cmd.output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout)
                    .chars()
                    .take(self.config.max_output_size)
                    .collect();
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(self.config.max_output_size)
                    .collect();

                ExecutionResult {
                    id,
                    success: output.status.success(),
                    stdout,
                    stderr,
                    exit_code: output.status.code().unwrap_or(-1),
                    execution_time_ms: start_time.elapsed().as_millis(),
                    error: None,
                }
            }
            Err(e) => ExecutionResult {
                id,
                success: false,
                stdout: String::new(),
                stderr: String::new(),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(e.to_string()),
            },
        }
    }

    async fn execute_powershell(
        &self,
        code: &str,
        _timeout: u64,
        env: &Option<HashMap<String, String>>,
    ) -> ExecutionResult {
        let id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        let mut cmd = Command::new("powershell");
        cmd.arg("-Command").arg(code);
        
        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let output = cmd.output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout)
                    .chars()
                    .take(self.config.max_output_size)
                    .collect();
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(self.config.max_output_size)
                    .collect();

                ExecutionResult {
                    id,
                    success: output.status.success(),
                    stdout,
                    stderr,
                    exit_code: output.status.code().unwrap_or(-1),
                    execution_time_ms: start_time.elapsed().as_millis(),
                    error: None,
                }
            }
            Err(e) => ExecutionResult {
                id,
                success: false,
                stdout: String::new(),
                stderr: String::new(),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(e.to_string()),
            },
        }
    }

    async fn execute_rust(
        &self,
        code: &str,
        _timeout: u64,
        env: &Option<HashMap<String, String>>,
    ) -> ExecutionResult {
        let id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        let source_path = self.temp_dir.join(format!("{}.rs", id));
        let exe_path = self.temp_dir.join(format!("sandbox_{}", id));

        if let Err(e) = std::fs::write(&source_path, code) {
            return ExecutionResult {
                id,
                success: false,
                stdout: String::new(),
                stderr: format!("Failed to write source: {}", e),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(e.to_string()),
            };
        }

        let compile_result = Command::new("rustc")
            .arg(&source_path)
            .arg("-o")
            .arg(&exe_path)
            .output();

        match compile_result {
            Ok(compile_output) => {
                if !compile_output.status.success() {
                    let stderr = String::from_utf8_lossy(&compile_output.stderr)
                        .chars()
                        .take(self.config.max_output_size)
                        .collect();
                    
                    std::fs::remove_file(&source_path).ok();
                    
                    return ExecutionResult {
                        id,
                        success: false,
                        stdout: String::new(),
                        stderr,
                        exit_code: compile_output.status.code().unwrap_or(-1),
                        execution_time_ms: start_time.elapsed().as_millis(),
                        error: Some("Compilation failed".to_string()),
                    };
                }

                let mut cmd = Command::new(&exe_path);
                
                if let Some(env_vars) = env {
                    for (key, value) in env_vars {
                        cmd.env(key, value);
                    }
                }

                let run_output = cmd.output();
                
                std::fs::remove_file(&source_path).ok();
                std::fs::remove_file(&exe_path).ok();

                match run_output {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout)
                            .chars()
                            .take(self.config.max_output_size)
                            .collect();
                        let stderr = String::from_utf8_lossy(&output.stderr)
                            .chars()
                            .take(self.config.max_output_size)
                            .collect();

                        ExecutionResult {
                            id,
                            success: output.status.success(),
                            stdout,
                            stderr,
                            exit_code: output.status.code().unwrap_or(-1),
                            execution_time_ms: start_time.elapsed().as_millis(),
                            error: None,
                        }
                    }
                    Err(e) => ExecutionResult {
                        id,
                        success: false,
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: -1,
                        execution_time_ms: start_time.elapsed().as_millis(),
                        error: Some(e.to_string()),
                    },
                }
            }
            Err(e) => {
                std::fs::remove_file(&source_path).ok();
                
                ExecutionResult {
                    id,
                    success: false,
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: -1,
                    execution_time_ms: start_time.elapsed().as_millis(),
                    error: Some(e.to_string()),
                }
            }
        }
    }

    async fn execute_go(
        &self,
        code: &str,
        _timeout: u64,
        env: &Option<HashMap<String, String>>,
    ) -> ExecutionResult {
        let id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        let source_path = self.temp_dir.join(format!("{}.go", id));

        if let Err(e) = std::fs::write(&source_path, code) {
            return ExecutionResult {
                id,
                success: false,
                stdout: String::new(),
                stderr: format!("Failed to write source: {}", e),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(e.to_string()),
            };
        }

        let mut cmd = Command::new("go");
        cmd.arg("run").arg(&source_path);
        
        if let Some(env_vars) = env {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        let output = cmd.output();
        
        std::fs::remove_file(&source_path).ok();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout)
                    .chars()
                    .take(self.config.max_output_size)
                    .collect();
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(self.config.max_output_size)
                    .collect();

                ExecutionResult {
                    id,
                    success: output.status.success(),
                    stdout,
                    stderr,
                    exit_code: output.status.code().unwrap_or(-1),
                    execution_time_ms: start_time.elapsed().as_millis(),
                    error: None,
                }
            }
            Err(e) => ExecutionResult {
                id,
                success: false,
                stdout: String::new(),
                stderr: String::new(),
                exit_code: -1,
                execution_time_ms: start_time.elapsed().as_millis(),
                error: Some(e.to_string()),
            },
        }
    }

    pub async fn get_history(&self) -> Vec<ExecutionResult> {
        self.execution_history.read().await.clone()
    }

    pub async fn clear_history(&self) {
        self.execution_history.write().await.clear();
    }
}
