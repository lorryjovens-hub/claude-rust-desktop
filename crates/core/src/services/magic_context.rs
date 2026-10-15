//! Explicit project consent for feeding recent conversation text into Magic Docs.
use super::ServiceContext;
use anyhow::{ensure, Result};
use serde::Serialize;
use std::path::Path;

pub fn enabled(context: &ServiceContext, project_id: &str) -> Result<bool> {
    Ok(context
        .setting::<bool>(&format!("magic-docs-conversations:{project_id}"))?
        .unwrap_or(false))
}
pub fn configure(
    context: &ServiceContext,
    root: &Path,
    project_id: &str,
    value: bool,
) -> Result<()> {
    let project = context
        .db
        .with_conn(|c| crate::db::project_repo::require_trusted(c, project_id))??;
    ensure!(
        !project.is_archived
            && project
                .workspace_path
                .as_deref()
                .and_then(|p| std::fs::canonicalize(p).ok())
                .as_deref()
                == Some(std::fs::canonicalize(root)?.as_path()),
        "Project does not match this workspace"
    );
    context.save_setting(&format!("magic-docs-conversations:{project_id}"), &value)
}

#[derive(Serialize)]
struct Message {
    session: String,
    role: String,
    content: String,
}

pub fn collect(
    context: &ServiceContext,
    root: &Path,
    project_id: Option<&str>,
    memories: Vec<String>,
) -> Result<Option<String>> {
    let root = std::fs::canonicalize(root)?;
    let projects = context
        .db
        .with_conn(crate::db::project_repo::list_projects)??;
    let mut messages = Vec::new();
    let mut remaining = 12000usize;
    for project in projects {
        if project_id.is_some_and(|id| id != project.id)
            || project.is_archived
            || project.folders.is_empty()
            || project.folders.iter().any(|f| !f.trusted)
            || project
                .workspace_path
                .as_deref()
                .and_then(|p| std::fs::canonicalize(p).ok())
                .as_ref()
                != Some(&root)
            || !enabled(context, &project.id)?
        {
            continue;
        }
        let rows = context.db.with_conn(|c| -> rusqlite::Result<Vec<Message>> {
            let mut stmt = c.prepare("SELECT m.conversation_id,m.role,substr(m.content,1,2000) FROM messages m JOIN conversations c ON c.id=m.conversation_id WHERE c.project_id=?1 AND c.archived=0 AND m.role IN ('user','assistant') ORDER BY m.created_at DESC,m.id DESC LIMIT 12")?;
            let values = stmt.query_map([&project.id], |r| Ok(Message { session:r.get(0)?, role:r.get(1)?, content:r.get(2)? }))?.collect();
            values
        })??;
        for mut message in rows {
            let mut end = message.content.len().min(remaining);
            while !message.content.is_char_boundary(end) {
                end -= 1;
            }
            message.content.truncate(end);
            remaining -= end;
            if !message.content.is_empty() {
                messages.push(message);
            }
            if remaining == 0 {
                break;
            }
        }
        if remaining == 0 {
            break;
        }
    }
    if memories.is_empty() && messages.is_empty() {
        return Ok(None);
    }
    let mut memories = memories;
    let mut budget = 12000usize;
    for memory in &mut memories {
        let mut end = memory.len().min(budget);
        while !memory.is_char_boundary(end) {
            end -= 1;
        }
        memory.truncate(end);
        budget -= end;
    }
    loop {
        let input = serde_json::to_string(
            &serde_json::json!({"memories":memories,"project_conversations":messages}),
        )?;
        if input.len() <= 32000 {
            return Ok(Some(input));
        }
        if !messages.is_empty() {
            messages.pop();
        } else if !memories.is_empty() {
            memories.pop();
        } else {
            return Ok(None);
        }
    }
}
