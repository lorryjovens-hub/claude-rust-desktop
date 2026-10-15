use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::db::{conversation_repo, project_repo};

pub const CODE_PROJECT_REQUIRED: &str = "Select a project before sending messages in Code mode";

/// Use persisted mode, never infer Code from a directory, model, or client prompt.
pub fn conversation_prompt_profile(conn: &Connection, id: &str) -> Result<crate::prompt::PromptProfile> {
    Ok(conversation_repo::get_conversation(conn, id)?
        .map(|conversation| crate::prompt::PromptProfile::from_workspace_mode(&conversation.workspace_mode))
        .unwrap_or_default())
}

pub fn conversation_project(conn: &Connection, id: &str) -> Result<Option<project_repo::ProjectRow>> {
    let conversation = conversation_repo::get_conversation(conn, id)?;
    if let Some(conversation) = conversation {
        if let Some(project_id) = conversation.project_id.filter(|id| !id.is_empty()) {
            let project = project_repo::require_trusted(conn, &project_id)?;
            ensure!(!project.is_archived, "Select an active project before sending messages");
            return Ok(Some(project));
        }
        ensure!(conversation.workspace_mode != "code", CODE_PROJECT_REQUIRED);
    }
    Ok(None)
}

/// A Home conversation always owns its working directory. Legacy paths and client
/// overrides must not turn an unassigned conversation into access to a source tree.
pub fn resolve_workspace(
    conn: &Connection,
    workspaces_root: &Path,
    id: &str,
    requested: Option<&str>,
) -> Result<(PathBuf, Option<project_repo::ProjectRow>)> {
    if let Some(mut project) = conversation_project(conn, id)? {
        let path = project.workspace_path.as_deref().context("Project has no working directory")?;
        ensure!(Path::new(path).is_absolute() && Path::new(path).is_dir(), "Project folder is unavailable");
        if let Some(worktree) = crate::worktree::session::resolve(conn, id, &project.id, Path::new(path))? {
            // The prompt and tools must agree on the actual working folder.
            project.workspace_path = Some(worktree.to_string_lossy().into_owned());
            if let Some(folder) = project.folders.first_mut() {
                folder.path = worktree.to_string_lossy().into_owned();
            }
            return Ok((worktree, Some(project)));
        }
        return Ok((PathBuf::from(path), Some(project)));
    }

    let conversation = conversation_repo::get_conversation(conn, id)?;
    // Internal jobs without a desktop conversation may explicitly supply their cwd.
    if conversation.is_none() {
        if let Some(path) = requested.filter(|path| !path.trim().is_empty()) {
            let path = PathBuf::from(path);
            ensure!(path.is_absolute() && path.is_dir(), "Working directory must be an existing absolute path");
            return Ok((path, None));
        }
    }

    ensure!(!id.is_empty() && id.len() <= 128 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'), "Invalid conversation workspace ID");
    ensure!(workspaces_root.is_absolute(), "Application workspace directory must be absolute");
    std::fs::create_dir_all(workspaces_root).context("Could not create application workspace directory")?;
    let root = std::fs::canonicalize(workspaces_root)?;
    let directory = root.join(id);
    std::fs::create_dir_all(&directory).context("Could not create conversation workspace")?;
    let directory = std::fs::canonicalize(directory)?;
    ensure!(directory.starts_with(&root) && directory != root, "Conversation workspace is outside the application workspace directory");
    if conversation.is_some() {
        conn.execute("UPDATE conversations SET workspace_path = ?1 WHERE id = ?2", params![directory.to_string_lossy(), id])?;
    }
    Ok((directory, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(mode: &str) -> (Connection, tempfile::TempDir) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::schema::SCHEMA_SQL).unwrap();
        conversation_repo::migrate_workspace_mode(&conn).unwrap();
        conversation_repo::insert_conversation(&conn, "session", None, None, None, Some("."), None, false, false, false, "now", "now", 0).unwrap();
        conversation_repo::set_workspace_mode(&conn, "session", mode).unwrap();
        (conn, tempfile::tempdir().unwrap())
    }

    #[test]
    fn prompt_profile_follows_saved_mode_and_defaults_for_internal_jobs() {
        for mode in ["chat", "work", "study", "code"] {
            let (conn, _root) = fixture(mode);
            let profile = conversation_prompt_profile(&conn, "session").unwrap();
            assert_eq!(profile == crate::prompt::PromptProfile::Code, mode == "code");
            assert_eq!(conversation_prompt_profile(&conn, "internal-job").unwrap(), crate::prompt::PromptProfile::Desktop);
        }
    }

    #[test]
    fn home_modes_get_persistent_private_directories_instead_of_legacy_or_requested_paths() {
        for mode in ["chat", "work", "study"] {
            let (conn, root) = fixture(mode);
            let cwd = std::env::current_dir().unwrap();
            let (path, project) = resolve_workspace(&conn, root.path(), "session", cwd.to_str()).unwrap();
            assert!(project.is_none());
            assert_eq!(path, std::fs::canonicalize(root.path()).unwrap().join("session"));
            assert_ne!(path, cwd);
            std::fs::write(path.join("notes.txt"), "keep me").unwrap();
            assert_eq!(resolve_workspace(&conn, root.path(), "session", Some(".")).unwrap().0, path);
            assert_eq!(std::fs::read_to_string(path.join("notes.txt")).unwrap(), "keep me");
            assert_eq!(conversation_repo::get_conversation(&conn, "session").unwrap().unwrap().workspace_path.as_deref(), path.to_str());
            assert_ne!(resolve_workspace(&conn, root.path(), "another", None).unwrap().0, path);
        }
    }

    #[test]
    fn code_cannot_bypass_project_requirement_with_a_workspace_override() {
        let (conn, root) = fixture("code");
        assert!(conversation_project(&conn, "session").unwrap_err().to_string().contains(CODE_PROJECT_REQUIRED));
        assert!(resolve_workspace(&conn, root.path(), "session", root.path().to_str()).is_err());
        assert!(!root.path().join("session").exists());
    }

    #[test]
    fn selected_project_must_exist_and_be_trusted_and_overrides_requested_cwd() {
        let (conn, root) = fixture("code");
        project_repo::insert_project(&conn, "project", "Project", None, None, None, false, "now", "now").unwrap();
        let folder = root.path().join("source");
        std::fs::create_dir(&folder).unwrap();
        let folders = vec![folder.to_string_lossy().into_owned()];
        project_repo::set_folders(&conn, "project", &folders).unwrap();
        conn.execute("UPDATE conversations SET project_id = 'project' WHERE id = 'session'", []).unwrap();
        assert!(resolve_workspace(&conn, root.path(), "session", None).is_err());
        project_repo::trust_folders(&conn, "project", &folders).unwrap();
        let (path, project) = resolve_workspace(&conn, root.path(), "session", Some(".")).unwrap();
        assert_eq!(std::fs::canonicalize(path).unwrap(), std::fs::canonicalize(folder).unwrap());
        assert_eq!(project.unwrap().id, "project");
        conn.execute("UPDATE projects SET is_archived = 1 WHERE id = 'project'", []).unwrap();
        assert!(resolve_workspace(&conn, root.path(), "session", None).is_err());
    }

    #[test]
    fn workspace_paths_cannot_escape_via_conversation_ids_or_relative_roots() {
        let (conn, root) = fixture("chat");
        for id in ["", ".", "..", "../source", "..\\source", "C:\\source", "a/b", "a:b"] {
            assert!(resolve_workspace(&conn, root.path(), id, None).is_err(), "{id}");
        }
        assert!(resolve_workspace(&conn, Path::new("."), "session", None).is_err());
    }
}
