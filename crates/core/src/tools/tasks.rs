//! Project-local durable task records. No background worker completion is fabricated.
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

pub(super) fn definitions() -> Vec<super::ToolDefinition> {
    [
        ("TaskCreate", "Create a durable project task.", json!({"name":{"type":"string"},"priority":{"type":"string"},"subject":{"type":"string"},"description":{"type":"string"},"activeForm":{"type":"string"},"metadata":{"type":"object"}}), vec![]),
        ("TaskGet", "Read a durable project task by ID.", json!({"id":{"type":"string"},"taskId":{"type":"string"}}),vec![]),
        ("TaskList", "List durable project tasks.", json!({}),vec![]),
        ("TaskUpdate", "Update a durable project task. Dependencies must exist and cannot form cycles.", json!({"id":{"type":"string"},"taskId":{"type":"string"},"name":{"type":"string"},"priority":{"type":"string"},"subject":{"type":"string"},"description":{"type":"string"},"status":{"type":"string","enum":["pending","in_progress","running","completed","failed","cancelled","deleted"]},"output":{"type":"string"},"owner":{"type":"string"},"activeForm":{"type":"string"},"metadata":{"type":"object"},"addBlockedBy":{"type":"array","items":{"type":"string"}},"addBlocks":{"type":"array","items":{"type":"string"}}}),vec![]),
        ("TodoWrite", "Replace the project's durable todo list.", json!({"todos":{"type":"array","items":{"type":"object","properties":{"content":{"type":"string"},"activeForm":{"type":"string"},"status":{"type":"string","enum":["pending","in_progress","completed"]}},"required":["content","status"]}}}),vec!["todos"]),
        ("TodoRead", "Read the project's durable todo list.",json!({}),vec![]),
        ("TaskOutput", "Read the saved output of a task record.",json!({"id":{"type":"string"}}),vec!["id"]),
        ("TaskStop", "Cancel a durable task record. This changes task metadata only and does not stop an executing worker; use host cancellation for running agents.",json!({"id":{"type":"string"}}),vec!["id"]),
        ("todo_write", "Manage durable todos using the legacy create/list/get/update/delete/complete/filter operations.",json!({"operation":{"type":"string","enum":["create","list","get","update","delete","complete","filter"]},"id":{"type":"string"},"content":{"type":"string"},"priority":{"type":"string","enum":["low","medium","high","urgent"]},"status":{"type":"string","enum":["pending","in_progress","completed","cancelled"]},"tags":{"type":"array","items":{"type":"string"}},"filter_status":{"type":"string"},"filter_priority":{"type":"string"},"filter_tags":{"type":"array","items":{"type":"string"}}}),vec!["operation"]),
    ].into_iter().map(|(name,description,properties,required)| super::ToolDefinition {name:name.into(),description:description.into(),input_schema:json!({"type":"object","properties":properties,"required":required})}).collect()
}

pub(super) fn execute(name: &str, mut input: Value, cwd: &str) -> Result<Value> {
    if input.get("taskId").is_none() {
        if let Some(id) = input.get("id").cloned() {
            input["taskId"] = id;
        }
    }
    if input.get("subject").is_none() {
        if let Some(subject) = input.get("name").cloned() {
            input["subject"] = subject;
        }
    }
    if name == "TaskStop" {
        input["status"] = json!("cancelled");
        let task = execute("TaskUpdate", input, cwd)?;
        return Ok(json!({"task":task,"record_cancelled":true,"worker_process_stopped":false}));
    }
    let directory = std::path::Path::new(cwd).canonicalize()?.join(".claude");
    // Apply the same path checks as file tools, including symlink resolution.
    super::resolve_path(&directory.join("tasks.sqlite3").to_string_lossy(), cwd)?;
    let readonly = matches!(name, "TaskGet" | "TaskList" | "TodoRead" | "TaskOutput")
        || (name == "todo_write"
            && matches!(input["operation"].as_str(), Some("list" | "filter" | "get")));
    let path = directory.join("tasks.sqlite3");
    if readonly && !path.exists() {
        return match name {
            "TaskList" => Ok(json!({"tasks":[]})),
            "TodoRead" => Ok(json!({"todos":[]})),
            "todo_write" if input["operation"] != "get" => Ok(json!({"todos":[]})),
            _ => bail!("Task not found"),
        };
    }
    if !readonly {
        std::fs::create_dir_all(&directory)?;
    }
    let mut db = if readonly {
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?
    } else {
        rusqlite::Connection::open(path)?
    };
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    if !readonly {
        db.execute_batch("CREATE TABLE IF NOT EXISTS tasks(id TEXT PRIMARY KEY, body TEXT NOT NULL); CREATE TABLE IF NOT EXISTS todo(singleton INTEGER PRIMARY KEY CHECK(singleton=1), body TEXT NOT NULL);")?;
    }
    let tx = db.transaction_with_behavior(if readonly {
        rusqlite::TransactionBehavior::Deferred
    } else {
        rusqlite::TransactionBehavior::Immediate
    })?;
    let get = |id: &str| -> Result<Value> {
        let body: Option<String> = tx
            .query_row("SELECT body FROM tasks WHERE id=?1", [id], |row| row.get(0))
            .optional()?;
        Ok(serde_json::from_str(
            &body.with_context(|| format!("Task not found: {id}"))?,
        )?)
    };
    let result = match name {
        "TaskCreate" => {
            let subject = input["subject"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .context("subject is required")?;
            let description = input["description"].as_str().unwrap_or("");
            let priority = input["priority"].as_str().unwrap_or("medium");
            if !matches!(priority, "low" | "medium" | "high" | "critical") {
                bail!("Invalid task priority");
            }
            let id = uuid::Uuid::new_v4().to_string();
            let mut task = json!({"id":id,"subject":subject,"description":description,"status":"pending","blockedBy":[],"blocks":[],"createdAt":chrono::Utc::now().to_rfc3339()});
            task["name"] = json!(subject);
            task["priority"] = json!(priority);
            task["output"] = Value::Null;
            for key in ["activeForm", "metadata"] {
                if let Some(value) = input.get(key) {
                    task[key] = value.clone();
                }
            }
            tx.execute(
                "INSERT INTO tasks(id,body) VALUES(?1,?2)",
                params![id, task.to_string()],
            )?;
            task
        }
        "TaskGet" => get(input["taskId"].as_str().context("taskId is required")?)?,
        "TaskOutput" => {
            let task = get(input["taskId"].as_str().context("id is required")?)?;
            json!({"id":task["id"],"output":task["output"]})
        }
        "TaskList" => {
            let mut statement = tx.prepare("SELECT body FROM tasks ORDER BY rowid")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            let mut tasks = vec![];
            for row in rows {
                tasks.push(serde_json::from_str::<Value>(&row?)?);
            }
            json!({"tasks":tasks})
        }
        "TaskUpdate" => {
            let id = input["taskId"].as_str().context("taskId is required")?;
            let mut task = get(id)?;
            if input["status"] == "deleted" {
                let mut statement = tx.prepare("SELECT id,body FROM tasks WHERE id!=?1")?;
                let rows = statement
                    .query_map([id], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                drop(statement);
                for (other_id, body) in rows {
                    let mut other: Value = serde_json::from_str(&body)?;
                    for field in ["blockedBy", "blocks"] {
                        other[field]
                            .as_array_mut()
                            .context("Invalid stored dependencies")?
                            .retain(|value| value != id);
                    }
                    tx.execute(
                        "UPDATE tasks SET body=?2 WHERE id=?1",
                        params![other_id, other.to_string()],
                    )?;
                }
                tx.execute("DELETE FROM tasks WHERE id=?1", [id])?;
                json!({"id":id,"deleted":true})
            } else {
                for key in [
                    "subject",
                    "description",
                    "status",
                    "owner",
                    "activeForm",
                    "output",
                ] {
                    if let Some(value) = input.get(key) {
                        if !value.is_string() {
                            bail!("{key} must be a string");
                        }
                        task[key] = value.clone();
                    }
                }
                if !matches!(
                    task["status"].as_str(),
                    Some(
                        "pending"
                            | "in_progress"
                            | "running"
                            | "completed"
                            | "failed"
                            | "cancelled"
                    )
                ) {
                    bail!("Invalid task status");
                }
                if let Some(metadata) = input.get("metadata") {
                    let updates = metadata.as_object().context("metadata must be an object")?;
                    if !task["metadata"].is_object() {
                        task["metadata"] = json!({});
                    }
                    for (key, value) in updates {
                        if value.is_null() {
                            task["metadata"].as_object_mut().unwrap().remove(key);
                        } else {
                            task["metadata"][key] = value.clone();
                        }
                    }
                }
                for (parameter, field, inverse) in [
                    ("addBlockedBy", "blockedBy", "blocks"),
                    ("addBlocks", "blocks", "blockedBy"),
                ] {
                    if let Some(values) = input.get(parameter) {
                        for value in values.as_array().context("Dependencies must be arrays")? {
                            let other_id =
                                value.as_str().context("Dependency must be a task ID")?;
                            if other_id == id {
                                bail!("Task cannot depend on itself");
                            }
                            let mut other = get(other_id)?;
                            // Traversing the opposite edge detects cycles before adding this edge.
                            let mut pending = vec![other_id.to_string()];
                            let mut visited = std::collections::HashSet::new();
                            while let Some(next) = pending.pop() {
                                if next == id {
                                    bail!("Task dependency cycle");
                                }
                                if visited.insert(next.clone()) {
                                    let node = get(&next)?;
                                    for edge in node[field]
                                        .as_array()
                                        .context("Invalid stored dependencies")?
                                    {
                                        pending.push(
                                            edge.as_str()
                                                .context("Invalid stored dependency")?
                                                .to_string(),
                                        );
                                    }
                                }
                            }
                            let edges = task[field]
                                .as_array_mut()
                                .context("Invalid stored dependencies")?;
                            if !edges.contains(value) {
                                edges.push(value.clone());
                            }
                            let edges = other[inverse]
                                .as_array_mut()
                                .context("Invalid stored dependencies")?;
                            if !edges.contains(&json!(id)) {
                                edges.push(json!(id));
                            }
                            tx.execute(
                                "UPDATE tasks SET body=?2 WHERE id=?1",
                                params![other_id, other.to_string()],
                            )?;
                        }
                    }
                }
                task["updatedAt"] = json!(chrono::Utc::now().to_rfc3339());
                tx.execute(
                    "UPDATE tasks SET body=?2 WHERE id=?1",
                    params![id, task.to_string()],
                )?;
                task
            }
        }
        "TodoWrite" => {
            let todos = input["todos"]
                .as_array()
                .context("todos must be an array")?;
            for todo in todos {
                if todo["content"].as_str().is_none_or(|s| s.trim().is_empty())
                    || !matches!(
                        todo["status"].as_str(),
                        Some("pending" | "in_progress" | "completed")
                    )
                {
                    bail!("Invalid todo content or status");
                }
            }
            tx.execute("INSERT INTO todo(singleton,body) VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET body=excluded.body",[json!(todos).to_string()])?;
            json!({"todos":todos})
        }
        "todo_write" => legacy_todos(&tx, &input)?,
        "TodoRead" => {
            let body: Option<String> = tx
                .query_row("SELECT body FROM todo WHERE singleton=1", [], |row| {
                    row.get(0)
                })
                .optional()?;
            json!({"todos":serde_json::from_str::<Value>(body.as_deref().unwrap_or("[]"))?})
        }
        _ => bail!("Unknown task tool: {name}"),
    };
    tx.commit()?;
    Ok(result)
}

fn legacy_todos(db: &rusqlite::Connection, input: &Value) -> Result<Value> {
    let body: Option<String> = db
        .query_row("SELECT body FROM todo WHERE singleton=1", [], |row| {
            row.get(0)
        })
        .optional()?;
    let mut todos: Vec<Value> = serde_json::from_str(body.as_deref().unwrap_or("[]"))?;
    let operation = input["operation"]
        .as_str()
        .context("operation is required")?;
    let result = match operation {
        "list" => json!({"todos":todos}),
        "filter" => {
            let filtered: Vec<_> = todos
                .iter()
                .filter(|todo| {
                    input
                        .get("filter_status")
                        .is_none_or(|value| todo["status"] == *value)
                        && input
                            .get("filter_priority")
                            .is_none_or(|value| todo["priority"] == *value)
                        && input["filter_tags"].as_array().is_none_or(|tags| {
                            tags.iter().all(|tag| {
                                todo["tags"]
                                    .as_array()
                                    .is_some_and(|saved| saved.contains(tag))
                            })
                        })
                })
                .cloned()
                .collect();
            json!({"todos":filtered})
        }
        "create" => {
            let mut todo = json!({"id":uuid::Uuid::new_v4().to_string(),"content":input["content"],"status":"pending","priority":input["priority"].as_str().unwrap_or("medium"),"tags":input.get("tags").cloned().unwrap_or(json!([])),"created_at":chrono::Utc::now().to_rfc3339()});
            if let Some(status) = input.get("status") {
                todo["status"] = status.clone();
            }
            validate_todo(&todo)?;
            todos.push(todo.clone());
            json!({"id":todo["id"],"todo":todo})
        }
        "get" | "update" | "delete" | "complete" => {
            let id = input["id"].as_str().context("id is required")?;
            let index = todos
                .iter()
                .position(|todo| todo["id"] == id)
                .context("Todo not found")?;
            match operation {
                "delete" => {
                    todos.remove(index);
                    json!({"id":id,"deleted":true})
                }
                "get" => json!({"todo":todos[index]}),
                _ => {
                    let todo = &mut todos[index];
                    if operation == "complete" {
                        todo["status"] = json!("completed");
                    } else {
                        for key in ["content", "status", "priority", "tags"] {
                            if let Some(value) = input.get(key) {
                                todo[key] = value.clone();
                            }
                        }
                    }
                    validate_todo(todo)?;
                    todo["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
                    json!({"todo":todo})
                }
            }
        }
        _ => bail!("Unknown todo operation"),
    };
    if !matches!(operation, "list" | "get" | "filter") {
        db.execute("INSERT INTO todo(singleton,body) VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET body=excluded.body",[serde_json::to_string(&todos)?])?;
    }
    Ok(result)
}
fn validate_todo(todo: &Value) -> Result<()> {
    if todo["content"].as_str().is_none_or(|s| s.trim().is_empty())
        || !matches!(
            todo["status"].as_str(),
            Some("pending" | "in_progress" | "completed" | "cancelled")
        )
        || !matches!(
            todo["priority"].as_str(),
            Some("low" | "medium" | "high" | "urgent")
        )
        || todo["tags"]
            .as_array()
            .is_none_or(|tags| tags.iter().any(|tag| !tag.is_string()))
    {
        bail!("Invalid todo fields");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_task_and_todo_parameters_remain_usable() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        let task = execute(
            "TaskCreate",
            json!({"name":"legacy","priority":"critical"}),
            cwd,
        )
        .unwrap();
        execute(
            "TaskUpdate",
            json!({"id":task["id"],"status":"running","output":"actual output"}),
            cwd,
        )
        .unwrap();
        assert_eq!(
            execute("TaskOutput", json!({"id":task["id"]}), cwd).unwrap()["output"],
            "actual output"
        );
        assert_eq!(
            execute("TaskStop", json!({"id":task["id"]}), cwd).unwrap()["worker_process_stopped"],
            false
        );
        let todo = execute(
            "todo_write",
            json!({"operation":"create","content":"old todo","priority":"urgent","tags":["rust"]}),
            cwd,
        )
        .unwrap();
        execute(
            "todo_write",
            json!({"operation":"complete","id":todo["id"]}),
            cwd,
        )
        .unwrap();
        assert_eq!(
            execute(
                "todo_write",
                json!({"operation":"filter","filter_tags":["rust"],"filter_status":"completed"}),
                cwd
            )
            .unwrap()["todos"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        execute(
            "todo_write",
            json!({"operation":"delete","id":todo["id"]}),
            cwd,
        )
        .unwrap();
        assert_eq!(
            execute("TodoRead", json!({}), cwd).unwrap()["todos"],
            json!([])
        );
    }
    #[test]
    fn tasks_persist_with_dependencies_and_failed_updates_roll_back() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        let a = execute(
            "TaskCreate",
            json!({"subject":"first","description":"one"}),
            cwd,
        )
        .unwrap();
        let b = execute(
            "TaskCreate",
            json!({"subject":"second","description":"two"}),
            cwd,
        )
        .unwrap();
        execute(
            "TaskUpdate",
            json!({"taskId":b["id"],"addBlockedBy":[a["id"]]}),
            cwd,
        )
        .unwrap();
        assert!(execute(
            "TaskUpdate",
            json!({"taskId":a["id"],"subject":"must roll back","addBlockedBy":[b["id"]]}),
            cwd
        )
        .is_err());
        let actual = execute("TaskGet", json!({"taskId":a["id"]}), cwd).unwrap();
        assert_eq!(actual["subject"], "first");
        assert_eq!(actual["blocks"], json!([b["id"]]));
        execute(
            "TaskUpdate",
            json!({"taskId":a["id"],"status":"deleted"}),
            cwd,
        )
        .unwrap();
        let actual = execute("TaskGet", json!({"taskId":b["id"]}), cwd).unwrap();
        assert_eq!(actual["blockedBy"], json!([]));
        assert_eq!(
            execute("TaskList", json!({}), cwd).unwrap()["tasks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn invalid_todos_do_not_replace_saved_list() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        let good = json!({"todos":[{"content":"task","status":"pending"}]});
        execute("TodoWrite", good.clone(), cwd).unwrap();
        assert!(execute(
            "TodoWrite",
            json!({"todos":[{"content":"bad","status":"made_up"}]}),
            cwd
        )
        .is_err());
        assert_eq!(execute("TodoRead", json!({}), cwd).unwrap(), good);
    }
}
