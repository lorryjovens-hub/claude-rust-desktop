use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub workspace_path: Option<String>,
    pub is_archived: bool,
    pub created_at: String,
    pub updated_at: String,
    pub folders: Vec<ProjectFolder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFolder {
    pub path: String,
    pub trusted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFileRow {
    pub id: String,
    pub project_id: String,
    pub file_name: Option<String>,
    pub file_path: Option<String>,
    pub file_size: Option<i64>,
    pub mime_type: Option<String>,
}

fn row_to_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        instructions: row.get(3)?,
        workspace_path: row.get(4)?,
        is_archived: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        folders: Vec::new(),
    })
}

fn row_to_project_file(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectFileRow> {
    Ok(ProjectFileRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        file_name: row.get(2)?,
        file_path: row.get(3)?,
        file_size: row.get(4)?,
        mime_type: row.get(5)?,
    })
}

pub fn insert_project(
    conn: &Connection,
    id: &str,
    name: &str,
    description: Option<&str>,
    instructions: Option<&str>,
    workspace_path: Option<&str>,
    is_archived: bool,
    created_at: &str,
    updated_at: &str,
) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO projects (id, name, description, instructions, workspace_path, is_archived, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    )?;
    stmt.execute(params![
        id,
        name,
        description,
        instructions,
        workspace_path,
        is_archived as i64,
        created_at,
        updated_at,
    ])?;
    Ok(())
}

pub fn get_project(conn: &Connection, id: &str) -> Result<Option<ProjectRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, name, description, instructions, workspace_path, is_archived, created_at, updated_at FROM projects WHERE id = ?1"
    )?;
    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(row) => {
            let mut project = row_to_project(row)?;
            project.folders = get_folders(conn, &project)?;
            Ok(Some(project))
        },
        None => Ok(None),
    }
}

pub fn list_projects(conn: &Connection) -> Result<Vec<ProjectRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, name, description, instructions, workspace_path, is_archived, created_at, updated_at FROM projects ORDER BY updated_at DESC"
    )?;
    let rows = stmt.query_map([], |row| row_to_project(row))?;
    let mut result = Vec::new();
    for row in rows {
        let mut project = row?;
        project.folders = get_folders(conn, &project)?;
        result.push(project);
    }
    Ok(result)
}

pub fn update_project(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    instructions: Option<&str>,
    workspace_path: Option<&str>,
    is_archived: Option<bool>,
) -> Result<()> {
    let current = get_project(conn, id)?
        .ok_or_else(|| anyhow::anyhow!("Project not found: {}", id))?;

    let name = name.unwrap_or(&current.name);
    let description = description.or(current.description.as_deref());
    let instructions = instructions.or(current.instructions.as_deref());
    let workspace_path = workspace_path.or(current.workspace_path.as_deref());
    let is_archived = is_archived.unwrap_or(current.is_archived);

    let mut stmt = conn.prepare_cached(
        "UPDATE projects SET name = ?1, description = ?2, instructions = ?3, workspace_path = ?4, is_archived = ?5, updated_at = ?6 WHERE id = ?7"
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    stmt.execute(params![
        name,
        description,
        instructions,
        workspace_path,
        is_archived as i64,
        now,
        id,
    ])?;
    Ok(())
}

pub fn delete_project(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("UPDATE conversations SET project_id = NULL, workspace_path = NULL WHERE project_id = ?1", params![id])?;
    conn.execute("DELETE FROM project_folders WHERE project_id = ?1", params![id])?;
    conn.execute("DELETE FROM project_files WHERE project_id = ?1", params![id])?;
    let mut stmt = conn.prepare_cached("DELETE FROM projects WHERE id = ?1")?;
    stmt.execute(params![id])?;
    Ok(())
}

fn get_folders(conn: &Connection, project: &ProjectRow) -> Result<Vec<ProjectFolder>> {
    let mut stmt = conn.prepare("SELECT path, trusted FROM project_folders WHERE project_id = ?1 ORDER BY sort_order")?;
    let rows = stmt.query_map(params![project.id], |row| Ok(ProjectFolder { path: row.get(0)?, trusted: row.get(1)? }))?;
    let mut folders = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if folders.is_empty() {
        if let Some(path) = project.workspace_path.as_ref().filter(|p| !p.is_empty()) {
            folders.push(ProjectFolder { path: path.clone(), trusted: false });
        }
    }
    Ok(folders)
}

pub fn normalize_folders(paths: &[String]) -> Result<Vec<String>> {
    anyhow::ensure!(!paths.is_empty(), "A project must have at least one folder");
    let mut result = Vec::new();
    for path in paths {
        let path = std::path::Path::new(path.trim());
        anyhow::ensure!(path.is_absolute() && path.is_dir(), "Folder must be an existing absolute directory: {}", path.display());
        let canonical = std::fs::canonicalize(path)?.to_string_lossy().to_string();
        // Windows canonical paths use the extended-length prefix; keep display paths readable.
        let canonical = canonical.strip_prefix("\\\\?\\UNC\\").map(|p| format!("\\\\{}", p))
            .unwrap_or_else(|| canonical.strip_prefix("\\\\?\\").unwrap_or(&canonical).to_string());
        if !result.contains(&canonical) { result.push(canonical); }
    }
    Ok(result)
}

pub fn set_folders(conn: &Connection, id: &str, paths: &[String]) -> Result<()> {
    let paths = normalize_folders(paths)?;
    let current = get_project(conn, id)?.ok_or_else(|| anyhow::anyhow!("Project not found"))?;
    conn.execute("DELETE FROM project_folders WHERE project_id = ?1", params![id])?;
    for (index, path) in paths.iter().enumerate() {
        let trusted = current.folders.iter().any(|f| f.path == *path && f.trusted);
        conn.execute("INSERT INTO project_folders (project_id, path, trusted, sort_order) VALUES (?1, ?2, ?3, ?4)", params![id, path, trusted, index as i64])?;
    }
    conn.execute("UPDATE projects SET workspace_path = ?1, updated_at = ?2 WHERE id = ?3", params![paths[0], chrono::Utc::now().to_rfc3339(), id])?;
    conn.execute("UPDATE conversations SET workspace_path = ?1 WHERE project_id = ?2 AND id NOT IN (SELECT conversation_id FROM conversation_worktrees)", params![paths[0], id])?;
    Ok(())
}

pub fn trust_folders(conn: &Connection, id: &str, paths: &[String]) -> Result<()> {
    let project = get_project(conn, id)?.ok_or_else(|| anyhow::anyhow!("Project not found"))?;
    let paths = normalize_folders(paths)?;
    anyhow::ensure!(paths.iter().all(|p| project.folders.iter().any(|f| f.path == *p)), "Folder is no longer part of this project");
    // Also materialize legacy projects before recording explicit consent.
    set_folders(conn, id, &project.folders.iter().map(|f| f.path.clone()).collect::<Vec<_>>())?;
    for path in paths {
        conn.execute("UPDATE project_folders SET trusted = 1 WHERE project_id = ?1 AND path = ?2", params![id, path])?;
    }
    Ok(())
}

pub fn require_trusted(conn: &Connection, id: &str) -> Result<ProjectRow> {
    let project = get_project(conn, id)?.ok_or_else(|| anyhow::anyhow!("Project not found"))?;
    anyhow::ensure!(!project.folders.is_empty(), "Add at least one folder to this project");
    anyhow::ensure!(project.folders.iter().all(|f| f.trusted), "Trust all project folders before starting a chat");
    let normalized = normalize_folders(&project.folders.iter().map(|f| f.path.clone()).collect::<Vec<_>>())?;
    anyhow::ensure!(normalized == project.folders.iter().map(|f| f.path.clone()).collect::<Vec<_>>(), "Project folders have changed; select and trust them again");
    Ok(project)
}

pub fn insert_project_file(
    conn: &Connection,
    id: &str,
    project_id: &str,
    file_name: Option<&str>,
    file_path: Option<&str>,
    file_size: Option<i64>,
    mime_type: Option<&str>,
) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO project_files (id, project_id, file_name, file_path, file_size, mime_type) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    )?;
    stmt.execute(params![id, project_id, file_name, file_path, file_size, mime_type])?;
    Ok(())
}

pub fn list_project_files(conn: &Connection, project_id: &str) -> Result<Vec<ProjectFileRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, project_id, file_name, file_path, file_size, mime_type FROM project_files WHERE project_id = ?1"
    )?;
    let rows = stmt.query_map(params![project_id], |row| row_to_project_file(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn delete_project_file(conn: &Connection, id: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached("DELETE FROM project_files WHERE id = ?1")?;
    stmt.execute(params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Connection, tempfile::TempDir, Vec<String>) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        conn.execute_batch(crate::db::schema::SCHEMA_SQL).unwrap();
        let root = tempfile::tempdir().unwrap();
        let paths = ["first", "second"].iter().map(|name| {
            let path = root.path().join(name);
            std::fs::create_dir(&path).unwrap();
            path.to_string_lossy().to_string()
        }).collect::<Vec<_>>();
        insert_project(&conn, "project", "Example", None, None, None, false, "now", "now").unwrap();
        (conn, root, paths)
    }

    #[test]
    fn projects_require_explicit_trust_and_keep_it_after_reload() {
        let (conn, _root, paths) = fixture();
        set_folders(&conn, "project", &paths).unwrap();
        assert!(require_trusted(&conn, "project").is_err());
        trust_folders(&conn, "project", &paths[..1]).unwrap();
        assert!(require_trusted(&conn, "project").is_err());
        trust_folders(&conn, "project", &paths[1..]).unwrap();
        let project = require_trusted(&conn, "project").unwrap();
        assert_eq!(project.folders.len(), 2);
        assert!(project.folders.iter().all(|f| f.trusted));
    }

    #[test]
    fn adding_and_readding_folders_requires_new_consent() {
        let (conn, _root, paths) = fixture();
        set_folders(&conn, "project", &paths[..1]).unwrap();
        trust_folders(&conn, "project", &paths[..1]).unwrap();
        set_folders(&conn, "project", &paths).unwrap();
        let project = get_project(&conn, "project").unwrap().unwrap();
        assert!(project.folders[0].trusted);
        assert!(!project.folders[1].trusted);
        trust_folders(&conn, "project", &paths).unwrap();
        set_folders(&conn, "project", &paths[..1]).unwrap();
        set_folders(&conn, "project", &paths).unwrap();
        assert!(!get_project(&conn, "project").unwrap().unwrap().folders[1].trusted);
    }

    #[test]
    fn cannot_remove_last_folder_or_trust_unknown_folder() {
        let (conn, _root, paths) = fixture();
        set_folders(&conn, "project", &paths[..1]).unwrap();
        assert!(set_folders(&conn, "project", &[]).is_err());
        assert!(trust_folders(&conn, "project", &paths[1..]).is_err());
        assert_eq!(get_project(&conn, "project").unwrap().unwrap().folders.len(), 1);
    }

    #[test]
    fn validates_folders_and_updates_linked_conversation_workspace() {
        let (conn, _root, paths) = fixture();
        assert!(normalize_folders(&["relative/path".to_string()]).is_err());
        set_folders(&conn, "project", &[paths[0].clone(), paths[0].clone()]).unwrap();
        assert_eq!(get_project(&conn, "project").unwrap().unwrap().folders.len(), 1);
        crate::db::conversation_repo::insert_conversation(&conn, "chat", None, None, None, Some(&paths[0]), Some("project"), false, false, false, "now", "now", 0).unwrap();
        set_folders(&conn, "project", &paths[1..]).unwrap();
        let chat = crate::db::conversation_repo::get_conversation(&conn, "chat").unwrap().unwrap();
        assert_eq!(chat.workspace_path, get_project(&conn, "project").unwrap().unwrap().workspace_path);
        delete_project(&conn, "project").unwrap();
        assert!(crate::db::conversation_repo::get_conversation(&conn, "chat").unwrap().unwrap().project_id.is_none());
    }
}
