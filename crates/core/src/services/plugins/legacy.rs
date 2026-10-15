//! Read-only conversion of historical plugin.json directories.
use super::*;
use rusqlite::TransactionBehavior;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    name: String,
    version: String,
    description: Option<String>,
    author: Option<String>,
    license: Option<String>,
    repository: Option<String>,
    main: String,
    commands: Vec<Command>,
    hooks: Vec<String>,
    dependencies: BTreeMap<String, String>,
    permissions: Vec<String>,
    enabled: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    name: String,
    description: String,
    usage: Option<String>,
    examples: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub id: String,
    pub preview: bool,
    pub imported: bool,
    pub skipped: bool,
    pub enabled: bool,
    pub backup: Option<String>,
    pub notes: Vec<String>,
}
fn collect(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, String>,
    total: &mut u64,
) -> Result<()> {
    for item in std::fs::read_dir(directory)? {
        let item = item?;
        let path = item.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "Plugin symbolic links are not supported"
        );
        ensure!(
            path.canonicalize()?.starts_with(root),
            "Plugin path escapes source directory"
        );
        if metadata.is_dir() {
            ensure!(
                path.components().count() - root.components().count() <= 16,
                "Plugin directory nesting exceeds limit"
            );
            collect(root, &path, files, total)?;
        } else {
            ensure!(metadata.is_file(), "Plugin contains a special file");
            *total += metadata.len();
            ensure!(
                *total <= 5_000_000 && files.len() < 100,
                "Plugin source exceeds package limits"
            );
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .context("Non-UTF8 plugin path")?
                .replace('\\', "/");
            ensure!(safe_path(&relative), "Invalid plugin source path");
            files.insert(
                relative,
                std::fs::read_to_string(path)
                    .context("Legacy script packages must contain UTF-8 text files")?,
            );
        }
    }
    Ok(())
}

pub fn import_legacy(context: &ServiceContext, source: &Path, preview: bool) -> Result<Report> {
    let root = source.canonicalize()?;
    ensure!(
        root.is_dir(),
        "Expected a legacy plugin directory containing plugin.json"
    );
    let mut files = BTreeMap::new();
    collect(&root, &root, &mut files, &mut 0)?;
    let manifest: Manifest =
        serde_json::from_str(files.get("plugin.json").context("plugin.json missing")?)?;
    ensure!(manifest.hooks.is_empty(), "Legacy manifest hooks have no executable handler mapping; convert hooks explicitly before import");
    ensure!(
        manifest.dependencies.is_empty(),
        "Legacy dependencies require explicit conversion; import does not install dependencies"
    );
    ensure!(manifest.permissions.is_empty(), "Legacy sandbox permissions are not enforced by script execution; convert to an explicit reviewed bundle before import");
    let interpreter = match Path::new(&manifest.main).extension().and_then(|s| s.to_str()) {
        Some("js" | "cjs" | "mjs") => "node",
        Some("py") => "python",
        _ => anyhow::bail!("Only JavaScript/Python legacy entrypoints can be converted; the old native/WASM loader defined no executable ABI"),
    };
    let mut commands = BTreeMap::new();
    for command in &manifest.commands {
        ensure!(
            commands
                .insert(
                    command.name.clone(),
                    PluginCommand {
                        description: command.description.clone(),
                        interpreter: interpreter.into(),
                        script: manifest.main.clone(),
                        args: Vec::new(),
                        legacy_arguments: true,
                    }
                )
                .is_none(),
            "Duplicate legacy command name"
        );
    }
    ensure!(
        !commands.is_empty(),
        "Legacy plugin has no executable commands"
    );
    let bundle = Bundle {
        id: manifest.name.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone().unwrap_or_default(),
        system_prompt: String::new(),
        commands,
        files,
    };
    bundle.validate()?;
    let payload = serde_json::to_string(&bundle)?;
    let backup = if preview {
        None
    } else {
        let directory = context.paths.data_dir.join("backups");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("plugins-{}.sqlite3", uuid::Uuid::new_v4()));
        context
            .db
            .with_conn(|c| c.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()]))??;
        Some(path.to_string_lossy().into_owned())
    };
    context.db.with_conn(|c| -> Result<Report> {
        let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
        let prior: Option<(String, bool)> = tx.query_row("SELECT bundle,enabled FROM installed_plugins WHERE id=?1", [&bundle.id], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
        let (skipped, enabled) = if let Some((prior, enabled)) = prior {
            ensure!(serde_json::from_str::<Value>(&prior)? == serde_json::from_str::<Value>(&payload)?, "Plugin ID conflicts with an installed package"); (true, enabled)
        } else { (false, manifest.enabled) };
        if !preview && !skipped {
            tx.execute("INSERT INTO installed_plugins(id,bundle,source,enabled) VALUES(?1,?2,?3,?4)", params![bundle.id,payload,root.to_string_lossy(),manifest.enabled])?;
        }
        if preview { tx.rollback()?; } else { tx.commit()?; }
        Ok(Report {id:bundle.id,preview,imported:!preview&&!skipped,skipped,enabled,backup,
            notes:vec!["Full original plugin.json is preserved. Each declared command executes main with --key=value string arguments; review this explicit conversion because historical manifest loading did not wire command handlers.".into(), "Import does not execute code. Running requires the usual explicit execution permission; code uses current OS permissions.".into()]})
    })?
}
