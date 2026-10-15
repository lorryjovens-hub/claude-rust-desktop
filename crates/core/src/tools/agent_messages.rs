//! Durable project-local agent mailboxes; no network messages are sent.
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

pub(super) fn definition() -> super::ToolDefinition {
    super::ToolDefinition {name:"SendMessage".into(),description:"Send or read persistent project-local agent messages. Operations: send, receive, pending_count, mark_read. Recipient labels are local routing names, not authenticated external identities. No network transmission.".into(),input_schema:json!({"type":"object","properties":{"operation":{"type":"string","enum":["send","receive","pending_count","mark_read"]},"to":{"type":"string"},"from":{"type":"string"},"content":{"type":"string"},"recipient":{"type":"string"},"unread_only":{"type":"boolean"},"message_id":{"type":"string"}},"required":["operation"]})}
}

pub fn execute_for_session(mut input: Value, cwd: &str, session_id: &str) -> Result<Value> {
    input["from"] = json!(session_id);
    input["recipient"] = json!(session_id);
    execute(input, cwd)
}

pub(super) fn execute(input: Value, cwd: &str) -> Result<Value> {
    let operation = input["operation"]
        .as_str()
        .context("operation is required")?;
    if !matches!(
        operation,
        "send" | "receive" | "pending_count" | "mark_read"
    ) {
        bail!("Unknown message operation");
    }
    let root = std::path::Path::new(cwd).canonicalize()?.join(".claude");
    let path = root.join("agent-messages.sqlite3");
    super::resolve_path(&path.to_string_lossy(), cwd)?;
    let readonly = matches!(operation, "receive" | "pending_count");
    if readonly && !path.exists() {
        return Ok(if operation == "receive" {
            json!({"messages":[]})
        } else {
            json!({"pending_count":0})
        });
    }
    if !readonly {
        std::fs::create_dir_all(&root)?;
    }
    let mut db = if readonly {
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?
    } else {
        rusqlite::Connection::open(path)?
    };
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    if !readonly {
        db.execute_batch("CREATE TABLE IF NOT EXISTS messages(id TEXT PRIMARY KEY,sender TEXT NOT NULL,recipient TEXT NOT NULL,content TEXT NOT NULL,created_at TEXT NOT NULL,read INTEGER NOT NULL DEFAULT 0); CREATE INDEX IF NOT EXISTS messages_recipient ON messages(recipient,read);")?;
    }
    let tx = db.transaction_with_behavior(if readonly {
        rusqlite::TransactionBehavior::Deferred
    } else {
        rusqlite::TransactionBehavior::Immediate
    })?;
    let recipient = input["recipient"].as_str().unwrap_or("agent");
    let result = match operation {
        "send" => {
            let to = input["to"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .context("to is required")?;
            let from = input["from"].as_str().unwrap_or("agent");
            let content = input["content"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("content is required")?;
            if to.len() > 256 || from.len() > 256 || content.len() > 64 * 1024 {
                bail!("Message exceeds recipient/sender/content limits");
            }
            let id = uuid::Uuid::new_v4().to_string();
            let created = chrono::Utc::now().to_rfc3339();
            tx.execute("INSERT INTO messages(id,sender,recipient,content,created_at) VALUES(?1,?2,?3,?4,?5)",params![id,from,to,content,created])?;
            json!({"id":id,"from":from,"to":to,"content":content,"created_at":created,"read":false})
        }
        "receive" => {
            let unread = input["unread_only"].as_bool().unwrap_or(true);
            let mut statement=tx.prepare("SELECT id,sender,recipient,content,created_at,read FROM messages WHERE recipient=?1 AND (?2=0 OR read=0) ORDER BY rowid LIMIT 1000")?;
            let rows=statement.query_map(params![recipient,unread],|row|Ok(json!({"id":row.get::<_,String>(0)?,"from":row.get::<_,String>(1)?,"to":row.get::<_,String>(2)?,"content":row.get::<_,String>(3)?,"created_at":row.get::<_,String>(4)?,"read":row.get::<_,bool>(5)?})))?;
            json!({"messages":rows.collect::<rusqlite::Result<Vec<_>>>()?})
        }
        "pending_count" => {
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM messages WHERE recipient=?1 AND read=0",
                [recipient],
                |row| row.get(0),
            )?;
            json!({"pending_count":count})
        }
        "mark_read" => {
            let id = input["message_id"]
                .as_str()
                .context("message_id is required")?;
            let exists: Option<String> = tx
                .query_row(
                    "SELECT id FROM messages WHERE id=?1 AND recipient=?2",
                    params![id, recipient],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_none() {
                bail!("Message not found in recipient mailbox");
            }
            tx.execute(
                "UPDATE messages SET read=1 WHERE id=?1 AND recipient=?2",
                params![id, recipient],
            )?;
            json!({"message_id":id,"read":true})
        }
        _ => unreachable!(),
    };
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_sessions_exchange_persistent_messages_and_ack_only_their_mailbox() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        let message = execute_for_session(
            json!({"operation":"send","from":"spoof","to":"child","content":"Inspect file"}),
            cwd,
            "parent",
        )
        .unwrap();
        assert_eq!(message["from"], "parent");
        let received = execute_for_session(
            json!({"operation":"receive","recipient":"parent"}),
            cwd,
            "child",
        )
        .unwrap();
        assert_eq!(received["messages"][0]["id"], message["id"]);
        assert!(execute_for_session(
            json!({"operation":"mark_read","message_id":message["id"]}),
            cwd,
            "parent"
        )
        .is_err());
        execute_for_session(
            json!({"operation":"mark_read","message_id":message["id"]}),
            cwd,
            "child",
        )
        .unwrap();
        assert_eq!(
            execute_for_session(json!({"operation":"pending_count"}), cwd, "child").unwrap()
                ["pending_count"],
            0
        );
        assert_eq!(
            execute_for_session(
                json!({"operation":"receive","unread_only":false}),
                cwd,
                "child"
            )
            .unwrap()["messages"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
