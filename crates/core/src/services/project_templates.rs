//! Project scaffolding shared by the standalone CLI and Desktop.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateFile {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub executable: bool,
}

/// Compatible with the historical ProjectTemplate JSON representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub language: String,
    pub files: Vec<TemplateFile>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPlan {
    pub destination: PathBuf,
    pub files: Vec<TemplateFile>,
    pub commands: Vec<String>,
    pub git: bool,
}

pub fn builtin_templates() -> Vec<ProjectTemplate> {
    let definitions: [(&str, &str, &[(&str, &str)]); 4] = [
        ("basic", "Generic", &[("README.md", "# {{project_name}}\n\nProject description.\n"), (".gitignore", "")]),
        ("rust", "Rust", &[("Cargo.toml", "[package]\nname = \"{{project_name}}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n"), ("src/main.rs", "fn main() {\n    println!(\"Hello, world!\");\n}\n"), (".gitignore", "/target\n")]),
        ("node", "JavaScript/TypeScript", &[("package.json", "{\n  \"name\": \"{{project_name}}\",\n  \"version\": \"1.0.0\",\n  \"main\": \"index.js\",\n  \"scripts\": {\"start\": \"node index.js\"}\n}\n"), ("index.js", "console.log('Hello, world!');\n"), (".gitignore", "node_modules/\n")]),
        ("python", "Python", &[("main.py", "def main():\n    print(\"Hello, world!\")\n\nif __name__ == \"__main__\":\n    main()\n"), ("requirements.txt", ""), (".gitignore", "__pycache__/\n*.pyc\n.env\n")]),
    ];
    definitions
        .into_iter()
        .map(|(name, language, files)| ProjectTemplate {
            name: name.into(),
            description: format!("{language} project"),
            language: language.into(),
            files: files
                .iter()
                .map(|(path, content)| TemplateFile {
                    path: (*path).into(),
                    content: (*content).into(),
                    executable: false,
                })
                .collect(),
            commands: vec![],
            variables: HashMap::new(),
        })
        .collect()
}

pub fn load_template(path: &Path) -> Result<ProjectTemplate> {
    let bytes = std::fs::read(path).context("Read project template")?;
    if bytes.len() > 8 * 1024 * 1024 {
        bail!("Project template exceeds 8 MiB");
    }
    Ok(serde_json::from_slice(&bytes).context("Invalid project template JSON")?)
}

fn validate_relative(path: &str) -> Result<()> {
    // Check both separators even on Unix so archives remain portable.
    if path.is_empty()
        || path.contains(':')
        || path.contains('\\')
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.ends_with('.') || p.ends_with(' '))
        || Path::new(path)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        bail!("Unsafe template path: {path}");
    }
    if path.split('/').any(|p| {
        let stem = p.split('.').next().unwrap_or("").to_ascii_uppercase();
        [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&stem.as_str())
    }) {
        bail!("Reserved template path: {path}");
    }
    Ok(())
}

pub fn preview(
    parent: &Path,
    name: &str,
    template: &ProjectTemplate,
    variables: &HashMap<String, String>,
    git: bool,
    vscode: bool,
) -> Result<ProjectPlan> {
    if name.is_empty()
        || name.len() > 100
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        || !name.as_bytes()[0].is_ascii_alphabetic()
    {
        bail!("Project name must start with a lowercase letter and contain only lowercase letters, digits, hyphens or underscores");
    }
    validate_relative(name)?;
    let parent = parent.canonicalize().context("Project parent must exist")?;
    if !parent.is_dir() {
        bail!("Project parent is not a directory");
    }
    let destination = parent.join(name);
    if destination.try_exists()? {
        bail!("Destination already exists: {}", destination.display());
    }
    let mut values = template.variables.clone();
    values.extend(variables.clone());
    values.insert("project_name".into(), name.into());
    let mut files = template.files.clone();
    if vscode {
        files.push(TemplateFile {
            path: ".vscode/settings.json".into(),
            content: "{\"files.exclude\":{\"**/.git\":true,\"**/.DS_Store\":true}}\n".into(),
            executable: false,
        });
        let configurations = match template.language.as_str() {
            "Rust" => {
                serde_json::json!([{ "type": "lldb", "request": "launch", "name": "Debug", "cargo": { "args": ["build", "--bin", name] } }])
            }
            "Python" => {
                serde_json::json!([{ "type": "debugpy", "request": "launch", "name": "Python: Current File", "program": "${file}" }])
            }
            _ => serde_json::json!([]),
        };
        files.push(TemplateFile {
            path: ".vscode/launch.json".into(),
            content: serde_json::to_string_pretty(
                &serde_json::json!({"version":"0.2.0", "configurations": configurations}),
            )?,
            executable: false,
        });
    }
    let mut paths = std::collections::HashSet::new();
    for file in &mut files {
        validate_relative(&file.path)?;
        if !paths.insert(file.path.to_lowercase()) {
            bail!("Duplicate template path: {}", file.path);
        }
        for (key, value) in &values {
            file.content = file.content.replace(&format!("{{{{{key}}}}}"), value);
        }
    }
    Ok(ProjectPlan {
        destination,
        files,
        commands: template
            .commands
            .iter()
            .map(|command| {
                let mut rendered = command.clone();
                for (key, value) in &values {
                    rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
                }
                rendered
            })
            .collect(),
        git,
    })
}

/// Commands and substituted variable values are shell code, requiring explicit opt-in.
/// Build in a private sibling directory, then publish without overwriting any project.
pub async fn create(plan: ProjectPlan, allow_commands: bool) -> Result<PathBuf> {
    if !plan.commands.is_empty() && !allow_commands {
        bail!("Template contains commands; review the preview and explicitly allow commands");
    }
    if plan.destination.try_exists()? {
        bail!("Destination already exists");
    }
    let parent = plan
        .destination
        .parent()
        .context("Missing project parent")?;
    let staging = tempfile::Builder::new()
        .prefix(".claude-project-")
        .tempdir_in(parent)?;
    for file in &plan.files {
        validate_relative(&file.path)?;
        let path = staging.path().join(&file.path);
        std::fs::create_dir_all(path.parent().context("Missing file parent")?)?;
        std::fs::write(&path, &file.content)?;
        #[cfg(unix)]
        if file.executable {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    if plan.git {
        let mut command = tokio::process::Command::new("git");
        command
            .arg("init")
            .current_dir(staging.path())
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null());
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let output = tokio::time::timeout(std::time::Duration::from_secs(60), command.output())
            .await
            .context("git init timed out")?
            .context("Start git init")?;
        if !output.status.success() {
            bail!(
                "git init failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    for command in &plan.commands {
        #[cfg(windows)]
        let mut child = {
            let mut c = tokio::process::Command::new("powershell.exe");
            c.args(["-NoProfile", "-NonInteractive", "-Command", command]);
            c
        };
        #[cfg(not(windows))]
        let mut child = {
            let mut c = tokio::process::Command::new("sh");
            c.args(["-c", command]);
            c
        };
        child
            .current_dir(staging.path())
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null());
        #[cfg(windows)]
        child.creation_flags(0x08000000);
        let output = tokio::time::timeout(std::time::Duration::from_secs(120), child.output())
            .await
            .context("Template command timed out")??;
        if !output.status.success() {
            bail!(
                "Template command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    // Reserve destination with create_dir (atomic no-clobber), then move its contents.
    // A competing initializer cannot overwrite an existing project.
    std::fs::create_dir(&plan.destination).context("Reserve project destination")?;
    let publish = (|| -> Result<()> {
        for entry in std::fs::read_dir(staging.path())? {
            let entry = entry?;
            std::fs::rename(entry.path(), plan.destination.join(entry.file_name()))?;
        }
        Ok(())
    })();
    if let Err(error) = publish {
        // Only remove the directory this invocation exclusively reserved.
        let _ = std::fs::remove_dir_all(&plan.destination);
        return Err(error);
    }
    Ok(plan.destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn all_builtin_templates_render_and_preserve_existing_projects() {
        let dir = tempfile::tempdir().unwrap();
        for template in builtin_templates() {
            let plan = preview(
                dir.path(),
                &template.name,
                &template,
                &HashMap::new(),
                false,
                true,
            )
            .unwrap();
            assert!(
                !plan.destination.exists(),
                "preview must not create a project"
            );
            let files = plan.files.clone();
            let path = create(plan.clone(), false).await.unwrap();
            for file in files {
                assert_eq!(
                    std::fs::read_to_string(path.join(file.path)).unwrap(),
                    file.content
                );
            }
            assert!(create(plan, false).await.is_err());
            assert!(preview(
                dir.path(),
                &template.name,
                &template,
                &HashMap::new(),
                false,
                false
            )
            .is_err());
        }
    }

    #[test]
    fn traversal_duplicates_and_invalid_names_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut template = builtin_templates().remove(0);
        for path in [
            "../outside",
            "C:/outside",
            "/outside",
            "a/../outside",
            "a\\outside",
            "CON.txt",
            "a./b",
        ] {
            template.files[0].path = path.into();
            assert!(
                preview(dir.path(), "safe", &template, &HashMap::new(), false, false).is_err(),
                "{path}"
            );
        }
        template.files[0].path = ".GITIGNORE".into();
        assert!(preview(dir.path(), "safe", &template, &HashMap::new(), false, false).is_err());
        for name in ["../escape", "bad\"name", "CON", "", "-flag", "bad name"] {
            assert!(preview(
                dir.path(),
                name,
                &builtin_templates()[0],
                &HashMap::new(),
                false,
                false
            )
            .is_err());
        }
    }

    #[tokio::test]
    async fn legacy_custom_commands_require_opt_in_and_failure_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("template.json");
        std::fs::write(&source, r#"{"name":"custom","language":"Generic","files":[{"path":"README.md","content":"{{project_name}} {{author}}","executable":false}],"commands":["exit 7"],"variables":{"author":"author"}}"#).unwrap();
        let template = load_template(&source).unwrap();
        let plan = preview(
            dir.path(),
            "example",
            &template,
            &HashMap::new(),
            false,
            false,
        )
        .unwrap();
        assert_eq!(plan.files[0].content, "example author");
        assert!(create(plan.clone(), false).await.is_err());
        assert!(!plan.destination.exists());
        assert!(create(plan.clone(), true).await.is_err());
        assert!(!plan.destination.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn concurrent_creation_has_exactly_one_winner() {
        let dir = tempfile::tempdir().unwrap();
        let plan = preview(
            dir.path(),
            "example",
            &builtin_templates()[0],
            &HashMap::new(),
            true,
            false,
        )
        .unwrap();
        let (first, second) = tokio::join!(create(plan.clone(), false), create(plan, false));
        assert_ne!(first.is_ok(), second.is_ok());
        assert!(dir.path().join("example/.git").is_dir());
        assert!(dir.path().join("example/README.md").is_file());
    }
}
