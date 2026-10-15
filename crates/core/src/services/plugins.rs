//! Versioned JSON bundles. Installation never executes plugin code.
use super::ServiceContext;
use anyhow::{ensure, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Component, Path},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
mod legacy;
pub use legacy::import_legacy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommand {
    pub description: String,
    pub interpreter: String,
    pub script: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Historical scripts receive string arguments as --key=value.
    #[serde(default)]
    pub legacy_arguments: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub id: String,
    pub version: String,
    pub description: String,
    /// Declarative system-prompt hook, loaded only while enabled.
    #[serde(default)]
    pub system_prompt: String,
    pub commands: BTreeMap<String, PluginCommand>,
    pub files: BTreeMap<String, String>,
}
#[derive(Debug, Serialize)]
pub struct Installed {
    pub id: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub source: String,
    pub commands: Vec<String>,
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
impl Bundle {
    pub fn validate(&self) -> Result<()> {
        ensure!(valid_name(&self.id), "Invalid plugin ID");
        semver::Version::parse(&self.version)?;
        ensure!(
            self.system_prompt.len() <= 16_000
                && self.files.len() <= 100
                && self.commands.len() <= 30,
            "Plugin limits exceeded"
        );
        ensure!(
            self.files.iter().all(|(name, _)| safe_path(name)),
            "Plugin path escapes package"
        );
        ensure!(
            self.files.values().map(String::len).sum::<usize>() <= 5_000_000,
            "Plugin files exceed 5 MB"
        );
        for (name, command) in &self.commands {
            ensure!(
                command.args.len() <= 64 && command.args.iter().all(|a| a.len() <= 4096),
                "Plugin argument limits exceeded"
            );
            ensure!(
                valid_name(name)
                    && matches!(command.interpreter.as_str(), "node" | "python")
                    && self.files.contains_key(&command.script)
                    && safe_path(&command.script),
                "Invalid plugin command {name}"
            );
        }
        Ok(())
    }
}
async fn source_bytes(source: &str) -> Result<Vec<u8>> {
    let bytes = if source.starts_with("https://") || source.starts_with("http://") {
        super::validate_endpoint(source)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let mut response = client.get(source).send().await?.error_for_status()?;
        ensure!(
            response.status().is_success(),
            "Unexpected plugin source status"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                bytes.len() + chunk.len() <= 6_000_000,
                "Plugin bundle exceeds 6 MB"
            );
            bytes.extend_from_slice(&chunk);
        }
        bytes
    } else {
        ensure!(
            std::fs::metadata(source)?.len() <= 6_000_000,
            "Plugin bundle exceeds 6 MB"
        );
        std::fs::read(source)?
    };
    Ok(bytes)
}
pub async fn source(source: &str) -> Result<Bundle> {
    let bundle: Bundle = serde_json::from_slice(&source_bytes(source).await?)?;
    bundle.validate()?;
    Ok(bundle)
}
pub async fn install(
    context: &ServiceContext,
    from: &str,
    update: Option<&str>,
) -> Result<Installed> {
    let bundle = source(from).await?;
    if let Some(id) = update {
        ensure!(
            bundle.id == id,
            "Update bundle ID differs from installed plugin"
        );
    }
    let payload = serde_json::to_string(&bundle)?;
    context.db.with_conn(|c| -> Result<()> {
        let tx = c.unchecked_transaction()?;
        let current: Option<(String, bool)> = tx
            .query_row(
                "SELECT bundle,enabled FROM installed_plugins WHERE id=?1",
                [&bundle.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match (update, current) {
            (None, None) => {
                tx.execute(
                    "INSERT INTO installed_plugins(id,bundle,source,enabled) VALUES(?1,?2,?3,1)",
                    params![bundle.id, payload, from],
                )?;
            }
            (Some(_), Some((old, _))) => {
                let old: Bundle = serde_json::from_str(&old)?;
                ensure!(
                    semver::Version::parse(&bundle.version)?
                        > semver::Version::parse(&old.version)?,
                    "Update must have a newer version"
                );
                tx.execute(
                    "UPDATE installed_plugins SET bundle=?1,source=?2 WHERE id=?3",
                    params![payload, from, bundle.id],
                )?;
            }
            (None, Some(_)) => anyhow::bail!("Plugin already installed; use update"),
            (Some(_), None) => anyhow::bail!("Plugin not installed"),
        }
        tx.commit()?;
        Ok(())
    })??;
    list(context)?
        .into_iter()
        .find(|p| p.id == bundle.id)
        .context("Installed plugin missing")
}
pub fn list(context: &ServiceContext) -> Result<Vec<Installed>> {
    context.db.with_conn(|c| -> Result<_> {
        let mut stmt =
            c.prepare("SELECT bundle,enabled,source FROM installed_plugins ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, bool>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|r| {
            let (raw, enabled, source) = r?;
            let b: Bundle = serde_json::from_str(&raw)?;
            Ok(Installed {
                id: b.id,
                version: b.version,
                description: b.description,
                enabled,
                source,
                commands: b.commands.keys().cloned().collect(),
            })
        })
        .collect()
    })?
}
pub fn enable(context: &ServiceContext, id: &str, enabled: bool) -> Result<()> {
    ensure!(
        context.db.with_conn(|c| c.execute(
            "UPDATE installed_plugins SET enabled=?1 WHERE id=?2",
            params![enabled, id]
        ))?? == 1,
        "Plugin not installed"
    );
    Ok(())
}
pub fn remove(context: &ServiceContext, id: &str) -> Result<()> {
    ensure!(
        context
            .db
            .with_conn(|c| c.execute("DELETE FROM installed_plugins WHERE id=?1", [id]))??
            == 1,
        "Plugin not installed"
    );
    Ok(())
}
fn enabled_bundle(context: &ServiceContext, id: &str) -> Result<Bundle> {
    let raw: String = context
        .db
        .with_conn(|c| {
            c.query_row(
                "SELECT bundle FROM installed_plugins WHERE id=?1 AND enabled=1",
                [id],
                |r| r.get(0),
            )
            .optional()
        })??
        .context("Plugin is disabled or not installed")?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn prompt_hooks(context: &ServiceContext) -> Result<String> {
    let mut text = String::new();
    for item in list(context)?.into_iter().filter(|p| p.enabled) {
        let bundle = enabled_bundle(context, &item.id)?;
        if !bundle.system_prompt.is_empty() {
            text.push_str(&format!(
                "\nEnabled plugin {}:\n{}\n",
                bundle.id, bundle.system_prompt
            ));
        }
    }
    Ok(text)
}

/// Callers must obtain the same execution permission as shell tools.
pub async fn run(
    context: &ServiceContext,
    id: &str,
    name: &str,
    input: Value,
    workspace: &Path,
) -> Result<Value> {
    let bundle = enabled_bundle(context, id)?;
    let command = bundle
        .commands
        .get(name)
        .context("Plugin command not registered")?;
    bundle.validate()?;
    let bytes = serde_json::to_vec(&input)?;
    ensure!(bytes.len() <= 1_048_576, "Plugin input exceeds 1 MiB");
    let mut legacy_args = Vec::new();
    if command.legacy_arguments {
        for (key, value) in input
            .as_object()
            .context("Legacy plugin input must be an object of strings")?
        {
            ensure!(valid_name(key), "Invalid legacy argument name");
            legacy_args.push(format!(
                "--{key}={}",
                value
                    .as_str()
                    .context("Legacy plugin argument values must be strings")?
            ));
        }
    }
    let directory = context.paths.data_dir.join("plugin-runs");
    std::fs::create_dir_all(&directory)?;
    let temporary = tempfile::tempdir_in(directory)?;
    for (path, content) in &bundle.files {
        let target = temporary.path().join(path);
        std::fs::create_dir_all(target.parent().unwrap())?;
        std::fs::write(target, content)?;
    }
    let mut process = tokio::process::Command::new(&command.interpreter);
    process
        .arg(temporary.path().join(&command.script))
        .args(&command.args)
        .args(legacy_args)
        .current_dir(std::fs::canonicalize(workspace)?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    process.creation_flags(0x08000000);
    let mut child = process.spawn().context("Plugin interpreter unavailable")?;
    let mut stdin = child.stdin.take().context("Plugin stdin unavailable")?;
    let stdout = child.stdout.take().context("Plugin stdout unavailable")?;
    let stderr = child.stderr.take().context("Plugin stderr unavailable")?;
    let bytes = if command.legacy_arguments {
        Vec::new()
    } else {
        bytes
    };
    let mut stdout = stdout.take(1_048_577);
    let mut stderr = stderr.take(1_048_577);
    let result = {
        let operation = async {
            let mut output = Vec::new();
            let mut errors = Vec::new();
            let input = async move {
                stdin.write_all(&bytes).await?;
                stdin.shutdown().await?;
                drop(stdin);
                Ok::<_, std::io::Error>(())
            };
            let (_, _, _, status) = tokio::try_join!(
                input,
                stdout.read_to_end(&mut output),
                stderr.read_to_end(&mut errors),
                child.wait()
            )?;
            ensure!(
                output.len() <= 1_048_576 && errors.len() <= 1_048_576,
                "Plugin output exceeds 1 MiB"
            );
            ensure!(
                status.success(),
                "Plugin failed: {}",
                String::from_utf8_lossy(&errors)
            );
            Ok::<_, anyhow::Error>(
                serde_json::json!({"stdout":String::from_utf8(output)?,"stderr":String::from_utf8(errors)?}),
            )
        };
        let operation = tokio::time::timeout(Duration::from_secs(60), operation);
        tokio::pin!(operation);
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            tokio::select! {result=&mut operation=>break result.unwrap_or_else(|_|Err(anyhow::anyhow!("Plugin timed out"))),_=tick.tick()=>{if enabled_bundle(context,id).map(|b|b.version!=bundle.version).unwrap_or(true){break Err(anyhow::anyhow!("Plugin disabled, removed or updated during execution"));}}}
        }
    };
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub version: String,
    pub description: String,
    pub source: String,
}
/// Operator-provided catalog; no bundled marketplace or automatic installation.
pub async fn search(registry: &str, query: &str) -> Result<Vec<CatalogEntry>> {
    let entries: Vec<CatalogEntry> = serde_json::from_slice(&source_bytes(registry).await?)?;
    ensure!(entries.len() <= 1000, "Catalog exceeds 1000 entries");
    for entry in &entries {
        ensure!(valid_name(&entry.id), "Invalid catalog plugin ID");
        semver::Version::parse(&entry.version)?;
        super::validate_endpoint(&entry.source)?;
    }
    let query = query.to_lowercase();
    Ok(entries
        .into_iter()
        .filter(|e| {
            format!("{} {}", e.id, e.description)
                .to_lowercase()
                .contains(&query)
        })
        .collect())
}
