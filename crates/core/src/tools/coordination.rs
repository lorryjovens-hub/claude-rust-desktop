//! Durable plans and teams; execution is delegated by the host to its existing tool loop.
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

static TEAM_LOCKS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, std::fs::File>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
fn team_lock(cwd: &str, id: &str) -> Result<(String, std::fs::File)> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        bail!("Invalid team ID");
    }
    let path = std::path::Path::new(cwd)
        .canonicalize()?
        .join(".claude")
        .join(format!("team-{id}.lock"));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    file.try_lock()
        .context("Team execution is active in another request or process")?;
    Ok((path.to_string_lossy().into_owned(), file))
}

pub(super) fn definitions() -> Vec<super::ToolDefinition> {
    vec![
        super::ToolDefinition{name:"team".into(),description:"Manage persistent project agent teams: create, get, list, delete, reset_task. Use run to execute pending tasks through the host's shared agent engine. Creation does not execute tasks.".into(),input_schema:json!({"type":"object","properties":{"operation":{"type":"string","enum":["create","get","list","delete","run","reset_task"]},"team_id":{"type":"string"},"task_id":{"type":"string"},"name":{"type":"string"},"tasks":{"type":"array","items":{"type":"object","properties":{"prompt":{"type":"string"},"description":{"type":"string"}},"required":["prompt"]}}},"required":["operation"]})},
        super::ToolDefinition{name:"plan_mode".into(),description:"Maintain durable project plans: create, enter, exit, get_current, add_step, update_step, list, get, abandon, decompose. Enter asks the host to restrict execution to plan mode. Exit never grants write permissions; only the user can change host permissions.".into(),input_schema:json!({"type":"object","properties":{"operation":{"type":"string","enum":["create","enter","exit","get_current","add_step","update_step","list","get","abandon","decompose"]},"plan_id":{"type":"string"},"step_id":{"type":"string"},"task":{"type":"string"},"steps":{"type":"array","items":{"type":"object","properties":{"description":{"type":"string"},"dependencies":{"type":"array","items":{"type":"string"}}},"required":["description"]}},"description":{"type":"string"},"dependencies":{"type":"array","items":{"type":"string"}},"status":{"type":"string","enum":["pending","blocked","ready","in_progress","completed","skipped"]},"result":{"type":"string"}},"required":["operation"]})}
    ]
}

fn connection(cwd: &str) -> Result<rusqlite::Connection> {
    let directory = std::path::Path::new(cwd).canonicalize()?.join(".claude");
    super::resolve_path(
        &directory.join("coordination.sqlite3").to_string_lossy(),
        cwd,
    )?;
    std::fs::create_dir_all(&directory)?;
    let db = rusqlite::Connection::open(directory.join("coordination.sqlite3"))?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,body TEXT NOT NULL,PRIMARY KEY(kind,id)); CREATE TABLE IF NOT EXISTS current_plan(singleton INTEGER PRIMARY KEY CHECK(singleton=1),id TEXT NOT NULL);")?;
    Ok(db)
}
fn get(db: &rusqlite::Connection, kind: &str, id: &str) -> Result<Value> {
    let body: Option<String> = db
        .query_row(
            "SELECT body FROM records WHERE kind=?1 AND id=?2",
            params![kind, id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(serde_json::from_str(
        &body.with_context(|| format!("{kind} not found: {id}"))?,
    )?)
}
fn put(db: &rusqlite::Connection, kind: &str, id: &str, body: &Value) -> Result<()> {
    db.execute("INSERT INTO records(kind,id,body) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET body=excluded.body",params![kind,id,body.to_string()])?;
    Ok(())
}
fn required<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    input[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .with_context(|| format!("{key} is required"))
}

pub fn execute(name: &str, input: Value, cwd: &str) -> Result<Value> {
    let mut db = connection(cwd)?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let operation = required(&input, "operation")?;
    let kind = if name == "team" {
        "team"
    } else if name == "plan_mode" {
        "plan"
    } else {
        bail!("Unknown coordination tool")
    };
    let id_key = if kind == "team" { "team_id" } else { "plan_id" };
    let result = match operation {
        "create" => {
            let id = uuid::Uuid::new_v4().to_string();
            let timestamp = chrono::Utc::now().to_rfc3339();
            let mut record = if kind == "team" {
                let name = required(&input, "name")?;
                let tasks = input["tasks"].as_array().context("tasks is required")?;
                if tasks.is_empty() || tasks.len() > 100 {
                    bail!("Team needs 1–100 tasks");
                }
                let tasks=tasks.iter().enumerate().map(|(index,task)|Ok(json!({"task_id":format!("task-{index}"),"prompt":required(task,"prompt")?,"description":task["description"].as_str().unwrap_or(""),"status":"pending","result":null,"error":null}))).collect::<Result<Vec<_>>>()?;
                json!({"team_id":id,"name":name,"tasks":tasks,"status":"active"})
            } else {
                let task = required(&input, "task")?;
                let steps = input["steps"].as_array().cloned().unwrap_or_default();
                let steps=steps.iter().enumerate().map(|(index,step)|Ok(json!({"step_id":format!("step-{index}"),"description":required(step,"description")?,"dependencies":step.get("dependencies").cloned().unwrap_or(json!([])),"status":"pending","result":null}))).collect::<Result<Vec<_>>>()?;
                validate_steps(&steps)?;
                json!({"plan_id":id,"task":task,"steps":steps,"status":"draft"})
            };
            record["created_at"] = json!(timestamp);
            record["updated_at"] = json!(timestamp);
            put(&tx, kind, &id, &record)?;
            record
        }
        "list" => {
            let mut statement =
                tx.prepare("SELECT body FROM records WHERE kind=?1 ORDER BY rowid")?;
            let rows = statement.query_map([kind], |row| row.get::<_, String>(0))?;
            let mut records = vec![];
            for row in rows {
                records.push(serde_json::from_str::<Value>(&row?)?);
            }
            json!({"items":records})
        }
        "get" => get(&tx, kind, required(&input, id_key)?)?,
        "delete" if kind == "team" => {
            let id = required(&input, id_key)?;
            let team = get(&tx, kind, id)?;
            if team["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|task| task["status"] == "running")
            {
                bail!("Cannot delete a team with running tasks; cancel execution first");
            }
            tx.execute(
                "DELETE FROM records WHERE kind=?1 AND id=?2",
                params![kind, id],
            )?;
            json!({"team_id":id,"deleted":true})
        }
        "reset_task" if kind == "team" => {
            let id = required(&input, id_key)?;
            let mut team = get(&tx, kind, id)?;
            let task_id = required(&input, "task_id")?;
            let task = team["tasks"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|task| task["task_id"] == task_id)
                .context("Task not found")?;
            if task["status"] == "running" {
                // A crashed host releases its OS lock; a live host cannot be reset.
                let _lock = team_lock(cwd, id)?;
            }
            task["status"] = json!("pending");
            task["result"] = Value::Null;
            task["error"] = Value::Null;
            team["status"] = json!("active");
            put(&tx, kind, id, &team)?;
            team
        }
        "get_current" if kind == "plan" => {
            let id: Option<String> = tx
                .query_row("SELECT id FROM current_plan WHERE singleton=1", [], |row| {
                    row.get(0)
                })
                .optional()?;
            if let Some(id) = id {
                get(&tx, kind, &id)?
            } else {
                Value::Null
            }
        }
        "exit" if kind == "plan" => {
            tx.execute("DELETE FROM current_plan WHERE singleton=1", [])?;
            json!({"exited":true,"permissions_unchanged":true})
        }
        "enter" | "abandon" | "add_step" | "update_step" | "decompose" if kind == "plan" => {
            let id = required(&input, id_key)?;
            let mut plan = get(&tx, kind, id)?;
            match operation {
                "enter" => {
                    if plan["status"] == "abandoned" {
                        bail!("Plan is abandoned");
                    }
                    plan["status"] = json!("active");
                    tx.execute("INSERT INTO current_plan(singleton,id) VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET id=excluded.id",[id])?;
                }
                "abandon" => {
                    plan["status"] = json!("abandoned");
                    tx.execute("DELETE FROM current_plan WHERE id=?1", [id])?;
                }
                "add_step" => {
                    let steps = plan["steps"].as_array_mut().unwrap();
                    steps.push(json!({"step_id":uuid::Uuid::new_v4().to_string(),"description":required(&input,"description")?,"dependencies":input.get("dependencies").cloned().unwrap_or(json!([])),"status":"pending","result":null}));
                    validate_steps(steps)?;
                }
                "update_step" => {
                    let steps = plan["steps"].as_array_mut().unwrap();
                    let step_id = required(&input, "step_id")?;
                    let step = steps
                        .iter_mut()
                        .find(|step| step["step_id"] == step_id)
                        .context("Step not found")?;
                    for key in ["description", "dependencies", "status", "result"] {
                        if let Some(value) = input.get(key) {
                            step[key] = value.clone();
                        }
                    }
                    validate_steps(steps)?;
                }
                "decompose" => {
                    let supplied = input["steps"]
                        .as_array()
                        .filter(|steps| !steps.is_empty())
                        .context("decompose requires explicit model/user-generated steps")?;
                    if plan["steps"].as_array().unwrap().iter().any(|step| {
                        matches!(step["status"].as_str(), Some("completed" | "in_progress"))
                    }) {
                        bail!("Cannot replace a plan with completed or running steps");
                    }
                    let steps=supplied.iter().enumerate().map(|(index,step)|Ok(json!({"step_id":format!("step-{index}"),"description":required(step,"description")?,"dependencies":step.get("dependencies").cloned().unwrap_or(json!([])),"status":"pending","result":null}))).collect::<Result<Vec<_>>>()?;
                    validate_steps(&steps)?;
                    plan["steps"] = json!(steps);
                }
                _ => unreachable!(),
            }
            plan["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
            put(&tx, kind, id, &plan)?;
            plan
        }
        "run" if kind == "team" => bail!("Team execution requires the shared agent host"),
        _ => bail!("Unsupported {kind} operation: {operation}"),
    };
    tx.commit()?;
    Ok(result)
}

fn validate_steps(steps: &[Value]) -> Result<()> {
    for step in steps {
        if !matches!(
            step["status"].as_str(),
            Some("pending" | "blocked" | "ready" | "in_progress" | "completed" | "skipped")
        ) {
            bail!("Invalid step status");
        }
        required(step, "description")?;
        let origin = required(step, "step_id")?;
        let mut pending = step["dependencies"]
            .as_array()
            .context("dependencies must be an array")?
            .clone();
        let mut seen = std::collections::HashSet::new();
        while let Some(next) = pending.pop() {
            let next = next.as_str().context("Dependency must be a step ID")?;
            if next == origin {
                bail!("Plan dependency cycle");
            }
            let target = steps
                .iter()
                .find(|s| s["step_id"] == next)
                .context("Plan dependency is missing")?;
            if seen.insert(next.to_string()) {
                pending.extend(
                    target["dependencies"]
                        .as_array()
                        .context("dependencies must be an array")?
                        .clone(),
                );
            }
        }
    }
    Ok(())
}

pub fn claim_team_task(cwd: &str, team_id: &str) -> Result<Option<Value>> {
    let mut db = connection(cwd)?;
    let (lock_key, lock) = team_lock(cwd, team_id)?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut team = get(&tx, "team", team_id)?;
    let tasks = team["tasks"].as_array_mut().context("Invalid team tasks")?;
    // One coordinator owns a team at a time; duplicate run calls cannot split tasks.
    for task in tasks.iter_mut().filter(|task| task["status"] == "running") {
        task["status"] = json!("failed");
        task["error"] = json!("Previous execution host exited; reset_task explicitly to retry");
    }
    let task = tasks.iter_mut().find(|task| task["status"] == "pending");
    let result = task.map(|task| {
        task["status"] = json!("running");
        task["host_pid"] = json!(std::process::id());
        task.clone()
    });
    if result.is_none() {
        team["status"] = json!(if tasks.iter().any(|task| task["status"] == "failed") {
            "failed"
        } else {
            "completed"
        });
    }
    put(&tx, "team", team_id, &team)?;
    tx.commit()?;
    if result.is_some() {
        TEAM_LOCKS
            .lock()
            .map_err(|_| anyhow::anyhow!("Team lock registry is poisoned"))?
            .insert(lock_key, lock);
    }
    Ok(result)
}

pub fn finish_team_task(
    cwd: &str,
    team_id: &str,
    task_id: &str,
    result: Option<&str>,
    error: Option<&str>,
) -> Result<()> {
    let mut db = connection(cwd)?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut team = get(&tx, "team", team_id)?;
    let tasks = team["tasks"].as_array_mut().context("Invalid team tasks")?;
    let task = tasks
        .iter_mut()
        .find(|task| task["task_id"] == task_id)
        .context("Task not found")?;
    if task["status"] != "running" {
        bail!("Task is not running");
    }
    task["status"] = json!(if error.is_some() {
        "failed"
    } else {
        "completed"
    });
    task["result"] = json!(result);
    task["error"] = json!(error);
    let status = if tasks.iter().any(|task| task["status"] == "failed") {
        "failed"
    } else if tasks.iter().all(|task| task["status"] == "completed") {
        "completed"
    } else {
        "active"
    };
    team["status"] = json!(status);
    team["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
    put(&tx, "team", team_id, &team)?;
    tx.commit()?;
    let lock_key = std::path::Path::new(cwd)
        .canonicalize()?
        .join(".claude")
        .join(format!("team-{team_id}.lock"))
        .to_string_lossy()
        .into_owned();
    TEAM_LOCKS
        .lock()
        .map_err(|_| anyhow::anyhow!("Team lock registry is poisoned"))?
        .remove(&lock_key);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn teams_claim_once_and_persist_real_results() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        let team=execute("team",json!({"operation":"create","name":"review","tasks":[{"prompt":"inspect"},{"prompt":"verify"}]}),cwd).unwrap();
        let id = team["team_id"].as_str().unwrap();
        let task = claim_team_task(cwd, id).unwrap().unwrap();
        assert!(claim_team_task(cwd, id).is_err());
        assert!(execute("team", json!({"operation":"delete","team_id":id}), cwd).is_err());
        finish_team_task(
            cwd,
            id,
            task["task_id"].as_str().unwrap(),
            Some("inspected"),
            None,
        )
        .unwrap();
        let task = claim_team_task(cwd, id).unwrap().unwrap();
        finish_team_task(
            cwd,
            id,
            task["task_id"].as_str().unwrap(),
            None,
            Some("cancelled"),
        )
        .unwrap();
        let team = execute("team", json!({"operation":"get","team_id":id}), cwd).unwrap();
        assert_eq!(team["status"], "failed");
        assert_eq!(team["tasks"][0]["result"], "inspected");
    }
    #[test]
    fn plans_persist_and_reject_missing_or_cyclic_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        let plan=execute("plan_mode",json!({"operation":"create","task":"build","steps":[{"description":"one"},{"description":"two","dependencies":["step-0"]}]}),cwd).unwrap();
        let id = plan["plan_id"].as_str().unwrap();
        execute("plan_mode", json!({"operation":"enter","plan_id":id}), cwd).unwrap();
        assert_eq!(
            execute("plan_mode", json!({"operation":"get_current"}), cwd).unwrap()["plan_id"],
            id
        );
        assert!(execute("plan_mode",json!({"operation":"update_step","plan_id":id,"step_id":"step-0","dependencies":["step-1"]}),cwd).is_err());
        let decomposed=execute("plan_mode",json!({"operation":"decompose","plan_id":id,"steps":[{"description":"A real supplied step"}]}),cwd).unwrap();
        assert_eq!(
            decomposed["steps"][0]["description"],
            "A real supplied step"
        );
        execute("plan_mode", json!({"operation":"exit"}), cwd).unwrap();
        assert!(
            execute("plan_mode", json!({"operation":"get_current"}), cwd)
                .unwrap()
                .is_null()
        );
    }
}
