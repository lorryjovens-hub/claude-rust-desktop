//! Git setup for project conversations. Git runs on the caller's blocking worker.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub(super) fn command(cwd: &Path, args: &[&str]) -> Result<Output> {
    let mut command = Command::new("git");
    command
        .current_dir(cwd)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .output()
        .context("Could not run Git. Check that Git is installed")
}

pub(super) fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = command(cwd, args)?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[derive(Serialize)]
pub struct Branches {
    pub is_repository: bool,
    pub current_branch: Option<String>,
    pub branches: Vec<String>,
}

pub fn branches(cwd: &Path) -> Result<Branches> {
    let probe = command(cwd, &["rev-parse", "--is-inside-work-tree"])?;
    if !probe.status.success() {
        // Permission/corruption errors should be visible, not mistaken for a non-Git folder.
        let error = String::from_utf8_lossy(&probe.stderr);
        ensure!(error.contains("not a git repository"), "{}", error.trim());
        return Ok(Branches {
            is_repository: false,
            current_branch: None,
            branches: vec![],
        });
    }
    ensure!(
        String::from_utf8_lossy(&probe.stdout).trim() == "true",
        "Select a Git working directory"
    );
    let current_branch = git(cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok();
    let branches = git(
        cwd,
        &[
            "for-each-ref",
            "--format=%(refname:strip=2)",
            "--sort=refname",
            "refs/heads/",
        ],
    )?
    .lines()
    .map(str::to_owned)
    .collect();
    Ok(Branches {
        is_repository: true,
        current_branch,
        branches,
    })
}

pub struct PreparedWorkspace {
    pub path: PathBuf,
    worktree: Option<(PathBuf, String)>,
}

impl PreparedWorkspace {
    pub fn save(
        &self,
        conn: &Connection,
        conversation_id: &str,
        project_id: &str,
        source: &str,
    ) -> Result<()> {
        if let Some((repo, branch)) = &self.worktree {
            conn.execute("INSERT INTO conversation_worktrees (conversation_id, project_id, source_path, repo_path, path, branch) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![conversation_id, project_id, source, repo.to_string_lossy(), self.path.to_string_lossy(), branch])?;
        }
        Ok(())
    }

    pub fn cleanup(&self) {
        if let Some((repo, branch)) = &self.worktree {
            // Never force-remove a directory: hooks may have left valuable changes.
            let root = git(&self.path, &["rev-parse", "--show-toplevel"])
                .unwrap_or_else(|_| self.path.to_string_lossy().into_owned());
            if git(repo, &["worktree", "remove", &root]).is_ok() {
                let _ = git(repo, &["branch", "-d", branch]);
            }
        }
    }
}

pub fn prepare(
    source: &Path,
    branch: Option<&str>,
    use_worktree: bool,
) -> Result<PreparedWorkspace> {
    if branch.is_none() && !use_worktree {
        return Ok(PreparedWorkspace {
            path: source.to_path_buf(),
            worktree: None,
        });
    }
    let state = branches(source)?;
    ensure!(
        state.is_repository,
        "This project folder is not a Git repository"
    );
    if let Some(branch) = branch {
        ensure!(
            state.branches.iter().any(|name| name == branch),
            "Selected branch no longer exists. Refresh the branch list"
        );
    }
    if !use_worktree {
        let branch = branch.context("Select a branch")?;
        if state.current_branch.as_deref() != Some(branch) {
            ensure!(
                git(source, &["status", "--porcelain"])?.is_empty(),
                "Commit or stash local changes before switching branches, or enable worktree"
            );
            git(source, &["switch", "--no-guess", branch])?;
        }
        return Ok(PreparedWorkspace {
            path: source.to_path_buf(),
            worktree: None,
        });
    }

    let base = branch
        .map(|name| format!("refs/heads/{name}"))
        .unwrap_or_else(|| "HEAD".into());
    // Resolve a commit first, including unborn-branch validation, before creating anything.
    let commit = git(
        source,
        &["rev-parse", "--verify", &format!("{base}^{{commit}}")],
    )?;
    let repo = PathBuf::from(git(source, &["rev-parse", "--show-toplevel"])?);
    let relative = source
        .canonicalize()?
        .strip_prefix(repo.canonicalize()?)?
        .to_path_buf();
    let common = PathBuf::from(git(
        source,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    // Keep generated worktrees outside the source tree so they do not appear as untracked files.
    let id = uuid::Uuid::new_v4().to_string();
    let path = common.join("desktop-worktrees").join(&id);
    std::fs::create_dir_all(path.parent().context("Invalid worktree path")?)?;
    let name = format!("session/{id}");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            &name,
            &path.to_string_lossy(),
            &commit,
        ],
    )?;
    // Preserve a project's subdirectory when it is nested inside the repository.
    let working_path = path.join(relative);
    let mut prepared = PreparedWorkspace {
        path,
        worktree: Some((repo, name)),
    };
    if !working_path.is_dir() {
        prepared.cleanup();
        anyhow::bail!("The project subdirectory does not exist on the selected branch");
    }
    // Persist both the worktree root and source-relative working directory via resolution below.
    prepared.path = working_path;
    Ok(prepared)
}

/// Only server-created, still-trusted worktrees may override a project's source directory.
pub fn resolve(
    conn: &Connection,
    id: &str,
    project_id: &str,
    source: &Path,
) -> Result<Option<PathBuf>> {
    let saved: Option<(String, String, String)> = conn.query_row(
        "SELECT source_path, repo_path, path FROM conversation_worktrees WHERE conversation_id = ?1 AND project_id = ?2",
        params![id, project_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional()?;
    let Some((original, repo, path)) = saved else {
        return Ok(None);
    };
    ensure!(
        Path::new(&original).canonicalize()? == source.canonicalize()?,
        "Project source folder changed; start a new conversation for this folder"
    );
    let path = PathBuf::from(path);
    ensure!(
        path.is_absolute() && path.is_dir(),
        "Conversation worktree is unavailable; restore it before continuing"
    );
    let common = |cwd: &Path| -> Result<PathBuf> {
        Ok(PathBuf::from(git(
            cwd,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?)
        .canonicalize()?)
    };
    ensure!(
        common(&path)? == common(Path::new(&repo))?,
        "Conversation worktree no longer belongs to this repository"
    );
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        git(root.path(), &["init", "-b", "main"]).unwrap();
        git(root.path(), &["config", "user.email", "test@example.com"]).unwrap();
        git(root.path(), &["config", "user.name", "Test"]).unwrap();
        std::fs::create_dir(root.path().join("nested")).unwrap();
        std::fs::write(root.path().join("nested/file.txt"), "main").unwrap();
        git(root.path(), &["add", "."]).unwrap();
        git(root.path(), &["commit", "-m", "initial"]).unwrap();
        git(root.path(), &["switch", "-c", "feature"]).unwrap();
        std::fs::write(root.path().join("nested/file.txt"), "feature").unwrap();
        git(root.path(), &["commit", "-am", "feature"]).unwrap();
        git(root.path(), &["switch", "main"]).unwrap();
        root
    }

    #[test]
    fn switches_selected_branch_but_preserves_dirty_files() {
        let root = repo();
        assert_eq!(
            branches(root.path()).unwrap().branches,
            vec!["feature", "main"]
        );
        prepare(root.path(), Some("feature"), false).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("nested/file.txt")).unwrap(),
            "feature"
        );
        std::fs::write(root.path().join("nested/file.txt"), "local edit").unwrap();
        assert!(prepare(root.path(), Some("main"), false).is_err());
        assert!(prepare(root.path(), Some("--force"), false).is_err());
        assert_eq!(
            std::fs::read_to_string(root.path().join("nested/file.txt")).unwrap(),
            "local edit"
        );
    }

    #[test]
    fn worktree_uses_selected_commit_and_resumes_with_project_context() {
        let root = repo();
        let source = root.path().join("nested");
        std::fs::write(source.join("file.txt"), "local edit").unwrap();
        let prepared = prepare(&source, Some("feature"), true).unwrap();
        assert_eq!(
            std::fs::read_to_string(prepared.path.join("file.txt")).unwrap(),
            "feature"
        );
        assert_eq!(
            branches(root.path()).unwrap().current_branch.as_deref(),
            Some("main")
        );
        assert_eq!(
            std::fs::read_to_string(source.join("file.txt")).unwrap(),
            "local edit"
        );

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::schema::SCHEMA_SQL).unwrap();
        use crate::db::{conversation_repo, project_repo};
        project_repo::insert_project(
            &conn, "project", "Project", None, None, None, false, "now", "now",
        )
        .unwrap();
        let folders = vec![source.to_string_lossy().into_owned()];
        project_repo::set_folders(&conn, "project", &folders).unwrap();
        project_repo::trust_folders(&conn, "project", &folders).unwrap();
        conversation_repo::insert_conversation(
            &conn,
            "chat",
            None,
            None,
            None,
            prepared.path.to_str(),
            Some("project"),
            false,
            false,
            false,
            "now",
            "now",
            0,
        )
        .unwrap();
        prepared
            .save(&conn, "chat", "project", source.to_str().unwrap())
            .unwrap();
        project_repo::set_folders(&conn, "project", &folders).unwrap();
        assert_eq!(
            conversation_repo::get_conversation(&conn, "chat")
                .unwrap()
                .unwrap()
                .workspace_path
                .as_deref(),
            prepared.path.to_str()
        );
        let (actual, project) =
            crate::native_engine::workspace::resolve_workspace(&conn, root.path(), "chat", None)
                .unwrap();
        assert_eq!(actual, prepared.path);
        assert_eq!(project.unwrap().folders[0].path, actual.to_string_lossy());
        let copied = conversation_repo::branch_conversation(&conn, "chat", None, None).unwrap();
        assert_eq!(
            resolve(&conn, &copied, "project", &source)
                .unwrap()
                .unwrap(),
            actual
        );
        prepared.cleanup();
        assert!(!actual.exists());
        assert!(resolve(&conn, "chat", "project", &source).is_err());
    }

    #[test]
    fn non_git_and_unborn_repositories_remain_usable_without_git_options() {
        let root = tempfile::tempdir().unwrap();
        assert!(!branches(root.path()).unwrap().is_repository);
        assert!(prepare(root.path(), None, false).is_ok());
        assert!(prepare(root.path(), None, true).is_err());
        git(root.path(), &["init", "-b", "main"]).unwrap();
        assert!(branches(root.path()).unwrap().branches.is_empty());
        assert!(prepare(root.path(), None, true).is_err());
        assert!(prepare(root.path(), None, false).is_ok());
    }
}
