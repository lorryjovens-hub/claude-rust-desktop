use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Request snapshots belong to the assistant message and follow its deletion lifecycle.
pub fn save_request_snapshots(conn: &Connection, message_id: &str, snapshots: &serde_json::Value) -> Result<()> {
    conn.execute("INSERT INTO message_trajectory (message_id, requests_json) VALUES (?1, ?2) ON CONFLICT(message_id) DO UPDATE SET requests_json = excluded.requests_json",
        params![message_id, serde_json::to_string(snapshots)?])?;
    if let Some(requests) = snapshots.as_array() {
        for request in requests { super::request_repo::save_request(conn, message_id, request)?; }
    }
    Ok(())
}

pub fn get_request_snapshots(conn: &Connection, message_id: &str) -> Result<serde_json::Value> {
    use rusqlite::OptionalExtension;
    let raw: Option<String> = conn.query_row("SELECT requests_json FROM message_trajectory WHERE message_id = ?1", params![message_id], |row| row.get(0)).optional()?;
    Ok(match raw { Some(raw) => serde_json::from_str(&raw)?, None => serde_json::json!([]) })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub thinking: Option<String>,
    pub created_at: String,
    pub is_compact_boundary: bool,
    pub sort_order: i64,
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        thinking: row.get(4)?,
        created_at: row.get(5)?,
        is_compact_boundary: row.get::<_, i64>(6)? != 0,
        sort_order: row.get(7)?,
    })
}

pub fn insert_message(
    conn: &Connection,
    id: &str,
    conversation_id: &str,
    role: &str,
    content: &str,
    thinking: Option<&str>,
    created_at: &str,
    is_compact_boundary: bool,
    sort_order: i64,
) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO messages (id, conversation_id, role, content, thinking, created_at, is_compact_boundary, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    )?;
    stmt.execute(params![
        id,
        conversation_id,
        role,
        content,
        thinking,
        created_at,
        is_compact_boundary as i64,
        sort_order,
    ])?;
    Ok(())
}

pub fn get_messages_by_conversation(conn: &Connection, conversation_id: &str) -> Result<Vec<MessageRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, conversation_id, role, content, thinking, created_at, is_compact_boundary, sort_order FROM messages WHERE conversation_id = ?1 ORDER BY sort_order ASC"
    )?;
    let rows = stmt.query_map(params![conversation_id], |row| row_to_message(row))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn get_message(conn: &Connection, id: &str) -> Result<Option<MessageRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, conversation_id, role, content, thinking, created_at, is_compact_boundary, sort_order FROM messages WHERE id = ?1"
    )?;
    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(Some(row_to_message(row)?)),
        None => Ok(None),
    }
}

pub fn delete_message(conn: &Connection, id: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached("DELETE FROM messages WHERE id = ?1")?;
    stmt.execute(params![id])?;
    Ok(())
}

pub fn delete_messages_from(conn: &Connection, conversation_id: &str, sort_order: i64) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "DELETE FROM messages WHERE conversation_id = ?1 AND sort_order >= ?2"
    )?;
    stmt.execute(params![conversation_id, sort_order])?;
    Ok(())
}

pub fn delete_messages_tail(conn: &Connection, conversation_id: &str, count: i64) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "DELETE FROM messages WHERE conversation_id = ?1 AND sort_order >= (SELECT MIN(sort_order) FROM (SELECT sort_order FROM messages WHERE conversation_id = ?1 ORDER BY sort_order DESC LIMIT ?2))"
    )?;
    stmt.execute(params![conversation_id, count])?;
    Ok(())
}

pub fn update_message_content(conn: &Connection, id: &str, content: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE messages SET content = ?1 WHERE id = ?2"
    )?;
    stmt.execute(params![content, id])?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRow {
    pub id: String,
    pub message_id: String,
    pub name: String,
    pub input: Option<String>,
    pub output: Option<String>,
    pub is_error: bool,
    pub sort_order: i64,
}

pub fn get_tool_calls_for_message(conn: &Connection, message_id: &str) -> Result<Vec<ToolCallRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, message_id, name, input, output, is_error, sort_order FROM tool_calls WHERE message_id = ?1 ORDER BY sort_order ASC"
    )?;
    let rows = stmt.query_map(params![message_id], |row| {
        Ok(ToolCallRow {
            id: row.get(0)?,
            message_id: row.get(1)?,
            name: row.get(2)?,
            input: row.get(3)?,
            output: row.get(4)?,
            is_error: row.get::<_, i64>(5)? != 0,
            sort_order: row.get(6)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn insert_tool_call(
    conn: &Connection,
    id: &str,
    message_id: &str,
    name: &str,
    input: Option<&str>,
    output: Option<&str>,
    is_error: bool,
    sort_order: i64,
) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT OR REPLACE INTO tool_calls (id, message_id, name, input, output, is_error, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    )?;
    stmt.execute(params![
        id,
        message_id,
        name,
        input,
        output,
        is_error as i64,
        sort_order,
    ])?;
    Ok(())
}

/// Save each result as it arrives, rather than waiting for the whole assistant turn.
pub fn save_stream_tool_result(conn: &Connection, conversation_id: &str, message_id: &str, content: &str, tool: &ToolCallRow) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("INSERT INTO messages (id, conversation_id, role, content, created_at, sort_order)
        VALUES (?1, ?2, 'assistant', ?3, ?4, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM messages WHERE conversation_id = ?2))
        ON CONFLICT(id) DO UPDATE SET content = excluded.content",
        params![message_id, conversation_id, content, chrono::Utc::now().to_rfc3339()])?;
    insert_tool_call(&tx, &tool.id, message_id, &tool.name, tool.input.as_deref(), tool.output.as_deref(), tool.is_error, tool.sort_order)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod stream_tool_tests {
    use super::*;

    #[test]
    fn results_survive_reload_before_turn_completion_and_updates_do_not_duplicate() {
        for mode in ["chat", "work", "study", "code"] {
            verify_stream_tool_persistence(mode);
        }
    }

    fn verify_stream_tool_persistence(mode: &str) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::schema::SCHEMA_SQL).unwrap();
        crate::db::conversation_repo::migrate_workspace_mode(&conn).unwrap();
        crate::db::conversation_repo::insert_conversation(&conn, "conv", None, None, None, None, None, false, false, false, "now", "now", 0).unwrap();
        crate::db::conversation_repo::set_workspace_mode(&conn, "conv", mode).unwrap();
        let mut tool = ToolCallRow { id: "assistant:call-1".into(), message_id: "assistant".into(), name: "Read".into(), input: Some("{}".into()), output: Some("first".into()), is_error: false, sort_order: 0 };
        save_stream_tool_result(&conn, "conv", "assistant", "Checking", &tool).unwrap();
        assert_eq!(get_tool_calls_for_message(&conn, "assistant").unwrap()[0].output.as_deref(), Some("first"));
        tool.output = Some("updated".into());
        save_stream_tool_result(&conn, "conv", "assistant", "Checking again", &tool).unwrap();
        let calls = get_tool_calls_for_message(&conn, "assistant").unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].output.as_deref(), Some("updated"));
        assert_eq!(get_messages_by_conversation(&conn, "conv").unwrap().len(), 1);
        assert_eq!(get_message(&conn, "assistant").unwrap().unwrap().content, "Checking again");
    }
}
