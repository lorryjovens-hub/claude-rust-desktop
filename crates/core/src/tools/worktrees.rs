use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub(super) fn definition() -> super::ToolDefinition {
    super::ToolDefinition { name:"worktree".into(), description:"Manage real Git worktrees: list, create, enter (resolve path), remove, prune, lock, unlock. Never force-removes dirty worktrees or deletes branches. Requires execution approval.".into(), input_schema:json!({"type":"object","properties":{"operation":{"type":"string","enum":["list","create","enter","remove","prune","lock","unlock"]},"path":{"type":"string"},"name":{"type":"string"},"branch":{"type":"string"}},"required":["operation"]}) }
}

async fn git(cwd: &str, args: &[String]) -> Result<String> {
    let mut command = tokio::process::Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(std::time::Duration::from_secs(60), command.output())
        .await
        .context("Git worktree operation timed out")??;
    if !output.status.success() {
        bail!(
            "Git worktree failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub(super) async fn execute(input: Value, cwd: &str) -> Result<Value> {
    let operation = input["operation"]
        .as_str()
        .context("operation is required")?;
    let listing = git(
        cwd,
        &[
            "worktree".into(),
            "list".into(),
            "--porcelain".into(),
            "-z".into(),
        ],
    )
    .await?;
    let mut worktrees = Vec::<Value>::new();
    for record in listing.split("\0\0").filter(|r| !r.is_empty()) {
        let mut item = json!({});
        for field in record.split('\0') {
            let (key, value) = field.split_once(' ').unwrap_or((field, "true"));
            item[if key == "worktree" { "path" } else { key }] = json!(value);
        }
        worktrees.push(item);
    }
    if operation == "list" {
        return Ok(json!({"worktrees":worktrees}));
    }
    if operation == "prune" {
        // Prune only expired administrative entries, matching Git's normal grace period.
        return Ok(
            json!({"output":git(cwd,&["worktree".into(),"prune".into(),"--verbose".into()]).await?}),
        );
    }
    let supplied = input["path"]
        .as_str()
        .or_else(|| input["name"].as_str())
        .context("path or name is required")?;
    let path = std::path::PathBuf::from(super::resolve_path(supplied, cwd)?);
    if operation == "create" {
        if path.exists() {
            bail!("Worktree destination already exists");
        }
        let branch = input["branch"].as_str().context("branch is required")?;
        if branch.starts_with('-') {
            bail!("Invalid branch name");
        }
        git(
            cwd,
            &["check-ref-format".into(), "--branch".into(), branch.into()],
        )
        .await?;
        let output = git(
            cwd,
            &[
                "worktree".into(),
                "add".into(),
                "-b".into(),
                branch.into(),
                "--".into(),
                path.to_string_lossy().into_owned(),
            ],
        )
        .await?;
        return Ok(json!({"path":path,"branch":branch,"output":output}));
    }
    let path = path.canonicalize()?;
    let known = worktrees
        .iter()
        .find(|w| {
            w["path"]
                .as_str()
                .and_then(|p| std::path::Path::new(p).canonicalize().ok())
                .is_some_and(|p| p == path)
        })
        .context("Path is not a registered worktree")?;
    if operation == "enter" {
        return Ok(
            json!({"path":path,"branch":known["branch"],"note":"Use this path as the working directory for subsequent operations. The host session directory is unchanged."}),
        );
    }
    if !matches!(operation, "remove" | "lock" | "unlock") {
        bail!("Unknown worktree operation");
    }
    let output = git(
        cwd,
        &[
            "worktree".into(),
            operation.into(),
            "--".into(),
            path.to_string_lossy().into_owned(),
        ],
    )
    .await?;
    Ok(json!({"path":path,"operation":operation,"output":output}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn worktrees_survive_new_calls_and_dirty_removal_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        git(cwd, &["init".into()]).await.unwrap();
        git(
            cwd,
            &[
                "-c".into(),
                "user.name=Test".into(),
                "-c".into(),
                "user.email=test@example.invalid".into(),
                "commit".into(),
                "--allow-empty".into(),
                "-m".into(),
                "initial".into(),
            ],
        )
        .await
        .unwrap();
        execute(
            json!({"operation":"create","path":"tree","branch":"feature"}),
            cwd,
        )
        .await
        .unwrap();
        let listed = execute(json!({"operation":"list"}), cwd).await.unwrap();
        assert_eq!(listed["worktrees"].as_array().unwrap().len(), 2);
        execute(json!({"operation":"lock","path":"tree"}), cwd)
            .await
            .unwrap();
        assert!(execute(json!({"operation":"remove","path":"tree"}), cwd)
            .await
            .is_err());
        execute(json!({"operation":"unlock","path":"tree"}), cwd)
            .await
            .unwrap();
        std::fs::write(dir.path().join("tree/dirty"), "keep").unwrap();
        assert!(execute(json!({"operation":"remove","path":"tree"}), cwd)
            .await
            .is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("tree/dirty")).unwrap(),
            "keep"
        );
        std::fs::remove_file(dir.path().join("tree/dirty")).unwrap();
        execute(json!({"operation":"remove","path":"tree"}), cwd)
            .await
            .unwrap();
        assert!(!dir.path().join("tree").exists());
    }
}
