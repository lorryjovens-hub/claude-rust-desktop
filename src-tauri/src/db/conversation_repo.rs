use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Copy history through the selected response, keeping the source session intact.
pub fn branch_conversation(conn: &Connection, source_id: &str, message_id: Option<&str>, message_index: Option<usize>) -> Result<String> {
    use super::message_repo;
    let transaction = conn.unchecked_transaction()?;
    let source = get_conversation(&transaction, source_id)?.ok_or_else(|| anyhow::anyhow!("Conversation not found"))?;
    let mut messages = message_repo::get_messages_by_conversation(&transaction, source_id)?;
    let index = message_id.and_then(|id| messages.iter().position(|message| message.id == id)).or(message_index);
    if let Some(index) = index {
        anyhow::ensure!(index < messages.len(), "Branch message not found");
        messages.truncate(index + 1);
    } else {
        anyhow::ensure!(message_id.is_none(), "Branch message not found");
    }
    let new_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let title = format!("{} (branch)", source.title.as_deref().unwrap_or("Conversation"));
    insert_conversation(&transaction, &new_id, Some(&title), source.model.as_deref(), source.provider.as_deref(),
        source.workspace_path.as_deref(), source.project_id.as_deref(), source.research_mode, false, false,
        &now, &now, messages.len() as i64)?;
    set_workspace_mode(&transaction, &new_id, &source.workspace_mode)?;
    transaction.execute("INSERT INTO conversation_worktrees (conversation_id, project_id, source_path, repo_path, path, branch) SELECT ?1, project_id, source_path, repo_path, path, branch FROM conversation_worktrees WHERE conversation_id = ?2", rusqlite::params![new_id, source_id])?;
    for message in messages {
        let new_message_id = uuid::Uuid::new_v4().to_string();
        message_repo::insert_message(&transaction, &new_message_id, &new_id, &message.role, &message.content,
            message.thinking.as_deref(), &message.created_at, message.is_compact_boundary, message.sort_order)?;
        let mut tool_ids = std::collections::HashMap::new();
        for tool in message_repo::get_tool_calls_for_message(&transaction, &message.id)? {
            let new_tool_id = uuid::Uuid::new_v4().to_string();
            message_repo::insert_tool_call(&transaction, &new_tool_id, &new_message_id, &tool.name,
                tool.input.as_deref(), tool.output.as_deref(), tool.is_error, tool.sort_order)?;
            tool_ids.insert(tool.id, new_tool_id);
        }
        let mut snapshots = message_repo::get_request_snapshots(&transaction, &message.id)?;
        if let Some(requests) = snapshots.as_array_mut() {
            for request in requests {
                request["id"] = serde_json::json!(uuid::Uuid::new_v4().to_string());
                if let Some(blocks) = request.get_mut("responseBlocks").and_then(|value| value.as_array_mut()) {
                    for block in blocks {
                        if let Some(new_tool_id) = block.get("id").and_then(|id| id.as_str()).and_then(|id| tool_ids.get(id)) {
                            block["id"] = serde_json::json!(new_tool_id);
                        }
                    }
                }
            }
        }
        message_repo::save_request_snapshots(&transaction, &new_message_id, &snapshots)?;
        transaction.execute("INSERT INTO attachments (id, message_id, file_name, file_type, mime_type, file_size, source)
            SELECT lower(hex(randomblob(16))), ?1, file_name, file_type, mime_type, file_size, source FROM attachments WHERE message_id = ?2",
            params![new_message_id, message.id])?;
    }
    transaction.commit()?;
    Ok(new_id)
}

#[cfg(test)]
mod branch_tests {
    use super::*;
    use crate::db::{message_repo, schema::SCHEMA_SQL};

    fn fixture() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        migrate_workspace_mode(&conn).unwrap();
        insert_conversation(&conn, "source", Some("Source"), Some("model"), Some("provider"), Some("D:/project"), None,
            false, false, false, "now", "now", 3).unwrap();
        set_workspace_mode(&conn, "source", "code").unwrap();
        for (index, role) in ["user", "assistant", "user"].iter().enumerate() {
            message_repo::insert_message(&conn, &format!("m{index}"), "source", role, &format!("content{index}"), None, "now", false, index as i64).unwrap();
        }
        message_repo::insert_tool_call(&conn, "tool", "m1", "Read", Some("{}"), Some("result"), false, 0).unwrap();
        message_repo::save_request_snapshots(&conn, "m1", &serde_json::json!([{
            "id": "request", "number": 1, "model": "model", "inputTokens": 100, "outputTokens": 20,
            "responseBlocks": [{"type": "tool_call", "id": "tool", "name": "Read"}]
        }])).unwrap();
        conn.execute("INSERT INTO attachments (id, message_id, file_name, source) VALUES ('attachment', 'm0', 'notes.txt', 'notes')", []).unwrap();
        conn
    }

    #[test]
    fn branch_includes_selected_answer_and_copies_metadata_without_reassigning_source_requests() {
        let conn = fixture();
        let id = branch_conversation(&conn, "source", Some("m1"), None).unwrap();
        let branch = get_conversation(&conn, &id).unwrap().unwrap();
        assert_eq!(branch.workspace_mode, "code");
        assert_eq!(branch.workspace_path.as_deref(), Some("D:/project"));
        assert_eq!(branch.provider.as_deref(), Some("provider"));
        assert_eq!(branch.message_count, 2);
        let messages = message_repo::get_messages_by_conversation(&conn, &id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, "content1");
        let tools = message_repo::get_tool_calls_for_message(&conn, &messages[1].id).unwrap();
        assert_eq!(tools[0].output.as_deref(), Some("result"));
        let snapshots = message_repo::get_request_snapshots(&conn, &messages[1].id).unwrap();
        assert_ne!(snapshots[0]["id"], "request");
        assert_eq!(snapshots[0]["outputTokens"], 20);
        assert_eq!(snapshots[0]["responseBlocks"][0]["id"], tools[0].id);
        let source_owner: String = conn.query_row("SELECT message_id FROM model_requests WHERE id = 'request'", [], |row| row.get(0)).unwrap();
        assert_eq!(source_owner, "m1");
        let attachments: i64 = conn.query_row("SELECT COUNT(*) FROM attachments WHERE message_id = ?1", params![messages[0].id], |row| row.get(0)).unwrap();
        assert_eq!(attachments, 1);
        assert_eq!(message_repo::get_messages_by_conversation(&conn, "source").unwrap().len(), 3);
    }

    #[test]
    fn branch_uses_optimistic_position_and_invalid_boundary_creates_nothing() {
        let conn = fixture();
        let id = branch_conversation(&conn, "source", Some("temporary-id"), Some(1)).unwrap();
        assert_eq!(message_repo::get_messages_by_conversation(&conn, &id).unwrap().len(), 2);
        assert!(branch_conversation(&conn, "source", Some("missing"), None).is_err());
        assert!(branch_conversation(&conn, "source", None, Some(99)).is_err());
        assert!(branch_conversation(&conn, "missing", None, None).is_err());
        assert_eq!(list_conversations(&conn).unwrap().len(), 2);
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceMode {
    #[default]
    Chat,
    #[serde(alias = "cowork")]
    Work,
    Study,
    Code,
}

impl WorkspaceMode {
    pub fn as_str(self) -> &'static str {
        match self { Self::Chat => "chat", Self::Work => "work", Self::Study => "study", Self::Code => "code" }
    }
}

pub fn migrate_workspace_mode(conn: &Connection) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('conversations') WHERE name = 'workspace_mode')",
        [], |row| row.get(0),
    )?;
    if !exists {
        conn.execute("ALTER TABLE conversations ADD COLUMN workspace_mode TEXT NOT NULL DEFAULT 'chat' CHECK (workspace_mode IN ('chat', 'work', 'study', 'code'))", [])?;
    } else {
        let schema: String = conn.query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'conversations'", [], |row| row.get(0))?;
        if schema.contains("'cowork'") && !schema.contains("'study'") {
            // Replace only the constrained column, keeping the table and all foreign-key
            // references intact. A savepoint also works inside the startup transaction.
            conn.execute_batch("SAVEPOINT workspace_mode_upgrade;")?;
            let result = conn.execute_batch(
                "ALTER TABLE conversations RENAME COLUMN workspace_mode TO legacy_workspace_mode;
                 ALTER TABLE conversations ADD COLUMN workspace_mode TEXT NOT NULL DEFAULT 'chat' CHECK (workspace_mode IN ('chat', 'work', 'study', 'code'));
                 UPDATE conversations SET workspace_mode = CASE legacy_workspace_mode WHEN 'cowork' THEN 'work' ELSE legacy_workspace_mode END;
                 ALTER TABLE conversations DROP COLUMN legacy_workspace_mode;"
            );
            if let Err(error) = result {
                conn.execute_batch("ROLLBACK TO workspace_mode_upgrade; RELEASE workspace_mode_upgrade;")?;
                return Err(error.into());
            }
            conn.execute_batch("RELEASE workspace_mode_upgrade;")?;
        }
    }
    Ok(())
}

pub fn set_workspace_mode(conn: &Connection, id: &str, mode: &str) -> Result<()> {
    let mode = if mode == "cowork" { "work" } else { mode };
    anyhow::ensure!(matches!(mode, "chat" | "work" | "study" | "code"), "Invalid workspace mode");
    conn.execute("UPDATE conversations SET workspace_mode = ?1 WHERE id = ?2", params![mode, id])?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRow {
    pub id: String,
    pub title: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub workspace_path: Option<String>,
    pub project_id: Option<String>,
    pub workspace_mode: String,
    pub research_mode: bool,
    pub pinned: bool,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
}

fn row_to_conversation(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationRow> {
    Ok(ConversationRow {
        id: row.get(0)?,
        title: row.get(1)?,
        model: row.get(2)?,
        provider: row.get(3)?,
        workspace_path: row.get(4)?,
        project_id: row.get(5)?,
        research_mode: row.get::<_, i64>(6)? != 0,
        pinned: row.get::<_, i64>(7)? != 0,
        archived: row.get::<_, i64>(8)? != 0,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        message_count: row.get(11)?,
        workspace_mode: row.get(12)?,
    })
}

pub fn insert_conversation(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    model: Option<&str>,
    provider: Option<&str>,
    workspace_path: Option<&str>,
    project_id: Option<&str>,
    research_mode: bool,
    pinned: bool,
    archived: bool,
    created_at: &str,
    updated_at: &str,
    message_count: i64,
) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO conversations (id, title, model, provider, workspace_path, project_id, research_mode, pinned, archived, created_at, updated_at, message_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
    )?;
    stmt.execute(params![
        id,
        title,
        model,
        provider,
        workspace_path,
        project_id,
        research_mode as i64,
        pinned as i64,
        archived as i64,
        created_at,
        updated_at,
        message_count,
    ])?;
    Ok(())
}

pub fn get_conversation(conn: &Connection, id: &str) -> Result<Option<ConversationRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, title, model, provider, workspace_path, project_id, research_mode, pinned, archived, created_at, updated_at, message_count, workspace_mode FROM conversations WHERE id = ?1"
    )?;
    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(Some(row_to_conversation(row)?)),
        None => Ok(None),
    }
}

pub fn list_conversations(conn: &Connection) -> Result<Vec<ConversationRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, title, model, provider, workspace_path, project_id, research_mode, pinned, archived, created_at, updated_at, message_count, workspace_mode FROM conversations ORDER BY updated_at DESC"
    )?;
    let rows = stmt.query_map([], |row| row_to_conversation(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn update_conversation_title(conn: &Connection, id: &str, title: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3"
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    stmt.execute(params![title, now, id])?;
    Ok(())
}

pub fn update_title_if_empty(conn: &Connection, id: &str, title: &str) -> Result<bool> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut stmt = conn.prepare_cached(
        "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3 AND (title IS NULL OR title = '')"
    )?;
    let rows = stmt.execute(params![title, now, id])?;
    Ok(rows > 0)
}

pub fn update_conversation_model(conn: &Connection, id: &str, model: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE conversations SET model = ?1, updated_at = ?2 WHERE id = ?3"
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    stmt.execute(params![model, now, id])?;
    Ok(())
}

pub fn update_conversation_timestamp(conn: &Connection, id: &str, updated_at: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2"
    )?;
    stmt.execute(params![updated_at, id])?;
    Ok(())
}

pub fn increment_message_count(conn: &Connection, id: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE conversations SET message_count = message_count + 1, updated_at = ?1 WHERE id = ?2"
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    stmt.execute(params![now, id])?;
    Ok(())
}

pub fn delete_conversation(conn: &Connection, id: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached("DELETE FROM conversations WHERE id = ?1")?;
    stmt.execute(params![id])?;
    Ok(())
}

pub fn pin_conversation(conn: &Connection, id: &str, pinned: bool) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE conversations SET pinned = ?1, updated_at = ?2 WHERE id = ?3"
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    stmt.execute(params![pinned as i64, now, id])?;
    Ok(())
}

pub fn archive_conversation(conn: &Connection, id: &str, archived: bool) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE conversations SET archived = ?1, updated_at = ?2 WHERE id = ?3"
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    stmt.execute(params![archived as i64, now, id])?;
    Ok(())
}

#[cfg(test)]
mod workspace_mode_tests {
    use super::*;

    #[test]
    fn legacy_migration_is_idempotent_and_preserves_history() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE conversations (id TEXT PRIMARY KEY, title TEXT); INSERT INTO conversations VALUES ('old', 'Existing chat');").unwrap();
        migrate_workspace_mode(&conn).unwrap();
        migrate_workspace_mode(&conn).unwrap();
        let value: (String, String) = conn.query_row("SELECT title, workspace_mode FROM conversations WHERE id = 'old'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(value, ("Existing chat".into(), "chat".into()));
        assert!(conn.execute("UPDATE conversations SET workspace_mode = 'invalid'", []).is_err());
    }

    #[test]
    fn modes_persist_independently_across_database_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("modes.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(crate::db::schema::SCHEMA_SQL).unwrap();
            for mode in ["chat", "work", "study", "code"] {
                insert_conversation(&conn, mode, Some("Same title"), None, None, None, None, false, false, false, "now", "now", 0).unwrap();
                set_workspace_mode(&conn, mode, mode).unwrap();
            }
            assert!(set_workspace_mode(&conn, "code", "invalid").is_err());
        }
        let conn = Connection::open(path).unwrap();
        let conversations = list_conversations(&conn).unwrap();
        assert_eq!(conversations.len(), 4);
        for mode in ["chat", "work", "study", "code"] {
            assert_eq!(conversations.iter().filter(|c| c.workspace_mode == mode).count(), 1);
            assert_eq!(get_conversation(&conn, mode).unwrap().unwrap().workspace_mode, mode);
        }
    }

    #[test]
    fn cowork_upgrade_preserves_related_history_inside_startup_transaction() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;
            CREATE TABLE conversations (id TEXT PRIMARY KEY, title TEXT, workspace_mode TEXT NOT NULL DEFAULT 'chat' CHECK (workspace_mode IN ('chat', 'cowork', 'code')));
            CREATE TABLE messages (id TEXT PRIMARY KEY, conversation_id TEXT REFERENCES conversations(id) ON DELETE CASCADE, content TEXT);
            CREATE INDEX conversation_titles ON conversations(title);
            INSERT INTO conversations VALUES ('old', 'Existing work', 'cowork');
            INSERT INTO messages VALUES ('message', 'old', 'Preserve this history');
            BEGIN;").unwrap();
        migrate_workspace_mode(&conn).unwrap();
        migrate_workspace_mode(&conn).unwrap();
        conn.execute_batch("COMMIT;").unwrap();
        let mode: String = conn.query_row("SELECT workspace_mode FROM conversations WHERE id = 'old'", [], |row| row.get(0)).unwrap();
        assert_eq!(mode, "work");
        let content: String = conn.query_row("SELECT content FROM messages WHERE conversation_id = 'old'", [], |row| row.get(0)).unwrap();
        assert_eq!(content, "Preserve this history");
        set_workspace_mode(&conn, "old", "study").unwrap();
        set_workspace_mode(&conn, "old", "cowork").unwrap();
        assert!(conn.execute("UPDATE conversations SET workspace_mode = 'invalid'", []).is_err());
        let index_count: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name = 'conversation_titles'", [], |row| row.get(0)).unwrap();
        assert_eq!(index_count, 1);
        assert_eq!(conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
    }

    #[test]
    fn mode_requests_accept_legacy_cowork_and_serialize_canonical_values() {
        for (input, expected) in [("chat", "chat"), ("cowork", "work"), ("work", "work"), ("study", "study"), ("code", "code")] {
            let mode: WorkspaceMode = serde_json::from_value(serde_json::json!(input)).unwrap();
            assert_eq!(mode.as_str(), expected);
            assert_eq!(serde_json::to_value(mode).unwrap(), serde_json::json!(expected));
        }
        assert!(serde_json::from_value::<WorkspaceMode>(serde_json::json!("invalid")).is_err());
    }
}

pub fn update_conversation(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    model: Option<&str>,
    provider: Option<&str>,
    workspace_path: Option<&str>,
    project_id: Option<&str>,
    research_mode: Option<bool>,
    pinned: Option<bool>,
    archived: Option<bool>,
    updated_at: Option<&str>,
    message_count: Option<i64>,
) -> Result<()> {
    let mut sql_parts = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(t) = title {
        sql_parts.push("title = ?");
        params.push(Box::new(t));
    }
    if let Some(m) = model {
        sql_parts.push("model = ?");
        params.push(Box::new(m));
    }
    if let Some(p) = provider {
        sql_parts.push("provider = ?");
        params.push(Box::new(p));
    }
    if let Some(wp) = workspace_path {
        sql_parts.push("workspace_path = ?");
        params.push(Box::new(wp));
    }
    if let Some(pi) = project_id {
        sql_parts.push("project_id = ?");
        params.push(Box::new(pi));
    }
    if let Some(rm) = research_mode {
        sql_parts.push("research_mode = ?");
        params.push(Box::new(rm as i64));
    }
    if let Some(pn) = pinned {
        sql_parts.push("pinned = ?");
        params.push(Box::new(pn as i64));
    }
    if let Some(a) = archived {
        sql_parts.push("archived = ?");
        params.push(Box::new(a as i64));
    }
    if let Some(ua) = updated_at {
        sql_parts.push("updated_at = ?");
        params.push(Box::new(ua));
    }
    if let Some(mc) = message_count {
        sql_parts.push("message_count = ?");
        params.push(Box::new(mc));
    }

    if sql_parts.is_empty() {
        return Ok(());
    }

    let sql = format!(
        "UPDATE conversations SET {} WHERE id = ?",
        sql_parts.join(", ")
    );
    
    let mut stmt = conn.prepare_cached(&sql)?;
    params.push(Box::new(id));
    stmt.execute(rusqlite::params_from_iter(params))?;
    Ok(())
}
