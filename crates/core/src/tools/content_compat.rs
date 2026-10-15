//! Deterministic legacy content tools and isolated project configuration documents.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Write, path::Path};

pub(super) fn definitions() -> Vec<super::ToolDefinition> {
    vec![
        super::ToolDefinition {name:"brief".into(),description:"Create deterministic text excerpts, first/last code lines or unified-diff counts. This is not model-based summarization; bulletpoints retains historical excerpt behavior.".into(),input_schema:json!({"type":"object","properties":{"operation":{"enum":["brief","code_summary","diff_summary"]},"content":{"type":"string"},"code":{"type":"string"},"diff":{"type":"string"},"format":{"enum":["short","medium","detailed","bulletpoints","bullets"]},"max_length":{"type":"integer","minimum":1,"maximum":100000},"include_keywords":{"type":"boolean"}},"required":["operation"]})},
        super::ToolDefinition {name:"SyntheticOutput".into(),description:"Return explicitly synthetic test/simulation output. Never represents execution or external verification.".into(),input_schema:json!({"type":"object","properties":{"content":{"type":"string"},"output_type":{"enum":["text","json","error"]},"metadata":{"type":"object"}},"required":["content"]})},
        super::ToolDefinition {name:"Config".into(),description:"Manage legacy project .claude.json / local .clauderc.json documents only. Does not configure the host, permissions, providers or plugin execution. User-home config is unsupported. Sensitive sections are redacted from reads; writes require edit permission.".into(),input_schema:json!({"type":"object","properties":{"operation":{"enum":["get","set","list","merge","sources","create"]},"source":{"enum":["local","project","default"]},"key":{"type":"string"},"value":{}},"required":["operation"]})},
    ]
}

pub(super) fn execute(name: &str, input: &Value, cwd: &str) -> Result<Value> {
    ensure!(
        serde_json::to_vec(input)?.len() <= 2_000_000,
        "Tool input exceeds 2 MB"
    );
    match name {
        "brief" => brief(input),
        "SyntheticOutput" => {
            let content = input["content"].as_str().context("content is required")?;
            let kind = input["output_type"].as_str().unwrap_or("text");
            ensure!(
                matches!(kind, "text" | "json" | "error"),
                "Invalid output_type"
            );
            let mut metadata = match input.get("metadata") {
                None | Some(Value::Null) => serde_json::Map::new(),
                Some(value) => value
                    .as_object()
                    .context("metadata must be an object")?
                    .clone(),
            };
            metadata.insert("synthetic".into(), json!(true));
            Ok(json!({"synthetic":true,"output_type":kind,"content":content,"metadata":metadata}))
        }
        "Config" => config(input, cwd),
        _ => bail!("Unknown content tool"),
    }
}

fn brief(input: &Value) -> Result<Value> {
    match input["operation"]
        .as_str()
        .context("operation is required")?
    {
        "brief" => {
            let content = input["content"]
                .as_str()
                .context("content is required")?
                .trim();
            let format = input["format"].as_str().unwrap_or("medium");
            let default = match format {
                "short" => 50,
                "medium" => 150,
                "detailed" => 300,
                "bulletpoints" | "bullets" => 200,
                _ => bail!("Invalid format"),
            };
            let max = match input.get("max_length") {
                None => default,
                Some(v) => v
                    .as_u64()
                    .context("max_length must be a positive integer")?,
            };
            ensure!(
                (1..=100_000).contains(&max),
                "max_length must be between 1 and 100000 characters"
            );
            let mut excerpt = content.chars().take(max as usize).collect::<String>();
            if content.chars().count() > max as usize {
                if let Some(space) = excerpt.rfind(' ') {
                    if space > 0 {
                        excerpt.truncate(space);
                    }
                }
                excerpt.push_str("...");
            }
            if input["include_keywords"].as_bool().unwrap_or(false) {
                let stop="the a an and or but in on at to for of with by from as is was are were be been being have has had do does did will would could should may might can this that these those i you he she it we they";
                let mut counts = BTreeMap::<String, usize>::new();
                for word in content
                    .split(|c: char| !c.is_alphanumeric())
                    .map(str::to_lowercase)
                    .filter(|w| w.chars().count() > 3 && !stop.split(' ').any(|s| s == w))
                {
                    *counts.entry(word).or_default() += 1;
                }
                let mut words = counts.into_iter().collect::<Vec<_>>();
                words.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                excerpt.push_str(&format!(
                    "\n\nKeywords: {}",
                    words
                        .into_iter()
                        .take(10)
                        .map(|p| p.0)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            Ok(json!({"success":true,"brief":excerpt,"method":"deterministic_excerpt"}))
        }
        "code_summary" => {
            let code = input["code"].as_str().context("code is required")?;
            let lines = code.lines().collect::<Vec<_>>();
            let summary = if lines.len() > 20 {
                format!("Code file with {} lines. First 5 lines:\n{}\n\n... ({} more lines) ...\n\nLast 5 lines:\n{}",lines.len(),lines[..5].join("\n"),lines.len()-10,lines[lines.len()-5..].join("\n"))
            } else {
                format!("Code file with {} lines:\n{}", lines.len(), code)
            };
            Ok(json!({"success":true,"summary":summary}))
        }
        "diff_summary" => {
            let diff = input["diff"].as_str().context("diff is required")?;
            let additions = diff
                .lines()
                .filter(|s| s.starts_with('+') && !s.starts_with("+++"))
                .count();
            let deletions = diff
                .lines()
                .filter(|s| s.starts_with('-') && !s.starts_with("---"))
                .count();
            Ok(
                json!({"success":true,"summary":format!("Diff summary: +{additions} additions, -{deletions} deletions"),"additions":additions,"deletions":deletions}),
            )
        }
        _ => bail!("Unknown brief operation"),
    }
}

fn path(root: &Path, source: &str) -> Result<std::path::PathBuf> {
    let name = match source {
        "local" => ".clauderc.json",
        "project" => ".claude.json",
        "user" => bail!(
            "User-home configuration is not supported; use explicit host configuration commands"
        ),
        _ => bail!("Invalid configuration source"),
    };
    let path = root.join(name);
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Configuration must be a regular file, not a link"
        );
        ensure!(
            path.canonicalize()?.parent() == Some(root),
            "Configuration escapes workspace"
        );
    }
    Ok(path)
}
fn load(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({"version":"1.0"}));
    }
    ensure!(
        std::fs::metadata(path)?.len() <= 1_000_000,
        "Configuration exceeds 1 MB"
    );
    let value: Value = serde_json::from_slice(&std::fs::read(path)?)
        .context("Invalid configuration JSON; no changes were saved")?;
    ensure!(value.is_object(), "Configuration must be an object");
    Ok(value)
}
fn redacted(value: &Value) -> Value {
    // Only schema-known noncredential fields leave the tool. MCP/environment/hooks may carry
    // credentials under arbitrary names, so redact whole sections instead of guessing secrets.
    let mut output = serde_json::Map::new();
    for (key, v) in value.as_object().into_iter().flatten() {
        let fields: &[&str] = match key.as_str() {
            "agent" => &["model", "temperature", "max_tokens"],
            "ui" => &["theme", "font_size"],
            "tools" => &["enabled", "disabled", "timeout_ms"],
            "plugins" => &["enabled", "disabled"],
            "permissions" => &["mode"],
            _ => &[],
        };
        let safe = if key == "version" {
            v.clone()
        } else if !fields.is_empty() {
            let mut section = serde_json::Map::new();
            if let Some(o) = v.as_object() {
                for (name, item) in o {
                    section.insert(
                        name.clone(),
                        if fields.contains(&name.as_str()) {
                            item.clone()
                        } else {
                            json!("[REDACTED]")
                        },
                    );
                }
            }
            Value::Object(section)
        } else {
            json!("[REDACTED]")
        };
        output.insert(key.clone(), safe);
    }
    Value::Object(output)
}
fn config(input: &Value, cwd: &str) -> Result<Value> {
    let root = Path::new(cwd).canonicalize()?;
    ensure!(root.is_dir(), "Workspace must be a directory");
    let operation = input["operation"]
        .as_str()
        .context("operation is required")?;
    let source = input["source"].as_str();
    if let Some(s) = source {
        ensure!(
            matches!(s, "local" | "project" | "default"),
            "User-home/unknown sources are unsupported; host configuration is separate"
        );
    }
    let note = "Legacy document only; does not alter active host configuration or permissions";
    match operation {
        "sources" | "list" => {
            let mut entries = Vec::new();
            for s in ["project", "local"] {
                let p = path(&root, s)?;
                let mut entry = json!({"source":s,"path":p,"exists":p.exists()});
                if operation == "list" {
                    entry["content"] = if p.exists() {
                        redacted(&load(&p)?)
                    } else {
                        Value::Null
                    };
                }
                entries.push(entry);
            }
            Ok(json!({"sources":entries,"note":note}))
        }
        "get" | "merge" => {
            let mut value = json!({"version":"1.0"});
            let sources = match source {
                Some("default") => Vec::new(),
                Some(s) => vec![s],
                None => vec!["project", "local"],
            };
            for s in sources {
                let p = path(&root, s)?;
                if p.exists() {
                    for (k, v) in load(&p)?.as_object().unwrap() {
                        if !v.is_null() {
                            value[k] = v.clone();
                        }
                    }
                }
            }
            let value = redacted(&value);
            let selected = input["key"]
                .as_str()
                .map(|k| value.get(k).cloned().unwrap_or(Value::Null))
                .unwrap_or(value);
            Ok(json!({"config":selected,"note":note}))
        }
        "set" | "create" => {
            let p = path(&root, source.unwrap_or("local"))?;
            let lock_path = root.join(".claude-config-edit.lock");
            if let Ok(m) = std::fs::symlink_metadata(&lock_path) {
                ensure!(
                    m.is_file() && !m.file_type().is_symlink(),
                    "Invalid configuration lock file"
                );
            }
            let lock = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(lock_path)?;
            lock.try_lock()
                .context("Configuration is being edited by another process")?;
            if operation == "create" {
                ensure!(!p.exists(), "Configuration already exists");
            }
            let mut value = load(&p)?;
            if let Some(key) = input["key"].as_str() {
                ensure!(
                    matches!(
                        key,
                        "version"
                            | "permissions"
                            | "tools"
                            | "agent"
                            | "ui"
                            | "mcp"
                            | "plugins"
                            | "hooks"
                    ),
                    "Unknown configuration key"
                );
                value[key] = input.get("value").context("value is required")?.clone();
            } else {
                ensure!(operation == "create", "key is required for set");
            }
            let bytes = serde_json::to_vec_pretty(&value)?;
            ensure!(bytes.len() <= 1_000_000, "Configuration exceeds 1 MB");
            let mut temp = tempfile::NamedTempFile::new_in(&root)?;
            temp.write_all(&bytes)?;
            temp.as_file().sync_all()?;
            if operation == "create" {
                temp.persist_noclobber(&p).map_err(|e| e.error)?;
            } else {
                temp.persist(&p).map_err(|e| e.error)?;
            }
            Ok(json!({"saved":true,"path":p,"note":note}))
        }
        _ => bail!("Unknown Config operation"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excerpt_is_utf8_safe_code_tail_is_ordered_and_diff_excludes_headers() {
        let result =
            brief(&json!({"operation":"brief","content":"你好世界🙂abcdef","max_length":3}))
                .unwrap();
        assert_eq!(result["brief"], "你好世...");
        assert!(brief(&json!({"operation":"brief","content":"x","max_length":-1})).is_err());
        let code = (1..=25)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let result = brief(&json!({"operation":"code_summary","code":code})).unwrap();
        assert!(result["summary"]
            .as_str()
            .unwrap()
            .ends_with("21\n22\n23\n24\n25"));
        let result =
            brief(&json!({"operation":"diff_summary","diff":"--- a\n+++ b\n-old\n+new\n context"}))
                .unwrap();
        assert_eq!(result["additions"], 1);
        assert_eq!(result["deletions"], 1);
    }
    #[test]
    fn synthetic_flag_cannot_be_overridden() {
        let result = execute(
            "SyntheticOutput",
            &json!({"content":"test","metadata":{"synthetic":false}}),
            ".",
        )
        .unwrap();
        assert_eq!(result["synthetic"], true);
        assert_eq!(result["metadata"]["synthetic"], true);
    }
    #[test]
    fn config_preserves_source_redacts_secrets_and_rejects_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        let p = dir.path().join(".claude.json");
        std::fs::write(&p,r#"{"ui":{"theme":"dark"},"mcp":{"servers":{"s":{"env":{"API_KEY":"do-not-disclose"}}}},"unknown":"secret"}"#).unwrap();
        let result = config(&json!({"operation":"get"}), cwd).unwrap();
        assert!(!result.to_string().contains("do-not-disclose"));
        assert!(!result.to_string().contains("secret"));
        config(
            &json!({"operation":"set","source":"project","key":"ui","value":{"theme":"light"}}),
            cwd,
        )
        .unwrap();
        let raw = std::fs::read_to_string(&p).unwrap();
        assert!(raw.contains("do-not-disclose"));
        assert!(config(&json!({"operation":"get","source":"user"}), cwd).is_err());
        std::fs::write(&p, "broken").unwrap();
        assert!(config(
            &json!({"operation":"set","source":"project","key":"ui","value":{}}),
            cwd
        )
        .is_err());
        assert_eq!(std::fs::read_to_string(p).unwrap(), "broken");
    }
}
