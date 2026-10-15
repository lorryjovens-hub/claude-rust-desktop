//! Durable recurring prompt schedules. A foreground CLI/Desktop host executes them.
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::str::FromStr;

pub(super) fn definition() -> super::ToolDefinition {
    super::ToolDefinition{name:"cron".into(),description:"Persist recurring model prompts with UTC cron schedules (five or six fields). Operations create/delete/list/get. A foreground cron service host must be running; creating a record does not start a hidden background process. Scheduled prompts generate text only; they do not execute tools.".into(),input_schema:json!({"type":"object","properties":{"operation":{"type":"string","enum":["create","delete","list","get"]},"cron_id":{"type":"string"},"schedule":{"type":"string"},"prompt":{"type":"string"},"description":{"type":"string"}},"required":["operation"]})}
}

fn connection(cwd: &str) -> Result<rusqlite::Connection> {
    let root = std::path::Path::new(cwd).canonicalize()?.join(".claude");
    super::resolve_path(&root.join("schedules.sqlite3").to_string_lossy(), cwd)?;
    std::fs::create_dir_all(&root)?;
    let db = rusqlite::Connection::open(root.join("schedules.sqlite3"))?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS schedules(id TEXT PRIMARY KEY,schedule TEXT NOT NULL,prompt TEXT NOT NULL,description TEXT NOT NULL,next_run INTEGER NOT NULL,last_run INTEGER,state TEXT NOT NULL DEFAULT 'pending',output TEXT,error TEXT); CREATE TABLE IF NOT EXISTS schedule_runs(id TEXT PRIMARY KEY,schedule_id TEXT NOT NULL,started_at INTEGER NOT NULL,finished_at INTEGER,state TEXT NOT NULL,output TEXT,error TEXT);")?;
    Ok(db)
}
pub fn next_run(expression: &str, after: chrono::DateTime<chrono::Utc>) -> Result<i64> {
    let fields = expression.split_whitespace().count();
    let expression = if fields == 5 {
        format!("0 {expression}")
    } else {
        expression.into()
    };
    let schedule = cron::Schedule::from_str(&expression).context("Invalid cron expression")?;
    Ok(schedule
        .after(&after)
        .next()
        .context("Cron expression has no future occurrence")?
        .timestamp())
}
fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(
        json!({"cron_id":row.get::<_,String>(0)?,"schedule":row.get::<_,String>(1)?,"prompt":row.get::<_,String>(2)?,"description":row.get::<_,String>(3)?,"next_run":row.get::<_,i64>(4)?,"last_run":row.get::<_,Option<i64>>(5)?,"state":row.get::<_,String>(6)?,"output":row.get::<_,Option<String>>(7)?,"error":row.get::<_,Option<String>>(8)?}),
    )
}

pub fn execute(input: Value, cwd: &str) -> Result<Value> {
    let operation = input["operation"]
        .as_str()
        .context("operation is required")?;
    let db = if matches!(operation, "list" | "get") {
        let path = std::path::Path::new(cwd)
            .canonicalize()?
            .join(".claude/schedules.sqlite3");
        super::resolve_path(&path.to_string_lossy(), cwd)?;
        if !path.exists() {
            return if operation == "list" {
                Ok(json!({"tasks":[]}))
            } else {
                bail!("Schedule not found")
            };
        }
        let db = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db
    } else {
        connection(cwd)?
    };
    match operation {
        "create" => {
            let schedule = input["schedule"].as_str().context("schedule is required")?;
            let prompt = input["prompt"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .context("prompt is required")?;
            if prompt.len() > 256 * 1024 {
                bail!("Scheduled prompt exceeds 256 KiB");
            }
            let next = next_run(schedule, chrono::Utc::now())?;
            let id = uuid::Uuid::new_v4().to_string();
            db.execute("INSERT INTO schedules(id,schedule,prompt,description,next_run) VALUES(?1,?2,?3,?4,?5)",params![id,schedule,prompt,input["description"].as_str().unwrap_or(""),next])?;
            Ok(json!({"cron_id":id,"next_run":next,"host_required":true}))
        }
        "list" => {
            let mut statement = db.prepare("SELECT * FROM schedules ORDER BY rowid")?;
            let rows = statement
                .query_map([], row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(json!({"tasks":rows}))
        }
        "get" => {
            let id = input["cron_id"].as_str().context("cron_id is required")?;
            db.query_row("SELECT * FROM schedules WHERE id=?1", [id], row)
                .optional()?
                .context("Schedule not found")
        }
        "delete" => {
            let id = input["cron_id"].as_str().context("cron_id is required")?;
            if db.execute(
                "DELETE FROM schedules WHERE id=?1 AND state!='running'",
                [id],
            )? != 1
            {
                bail!("Schedule is missing or running; stop its host before deleting");
            }
            Ok(json!({"cron_id":id,"deleted":true}))
        }
        _ => bail!("Unsupported cron operation"),
    }
}

pub struct ClaimedPrompt {
    pub id: String,
    pub prompt: String,
    pub run_id: String,
    cwd: String,
    finished: bool,
}
impl ClaimedPrompt {
    pub fn finish(mut self, output: Option<&str>, error: Option<&str>) -> Result<()> {
        self.record(output, error)?;
        self.finished = true;
        Ok(())
    }
    fn record(&self, output: Option<&str>, error: Option<&str>) -> Result<()> {
        let mut db = connection(&self.cwd)?;
        let tx = db.transaction()?;
        let state = if error.is_some() {
            "failed"
        } else {
            "completed"
        };
        tx.execute(
            "UPDATE schedules SET state=?2,output=?3,error=?4 WHERE id=?1",
            params![self.id, state, output, error],
        )?;
        tx.execute(
            "UPDATE schedule_runs SET finished_at=?2,state=?3,output=?4,error=?5 WHERE id=?1",
            params![
                self.run_id,
                chrono::Utc::now().timestamp(),
                state,
                output,
                error
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
}
impl Drop for ClaimedPrompt {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.record(None, Some("Scheduled request cancelled or host stopped"));
        }
    }
}

pub fn claim_due(cwd: &str) -> Result<Option<ClaimedPrompt>> {
    let mut db = connection(cwd)?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let now = chrono::Utc::now();
    let due=tx.query_row("SELECT id,schedule,prompt FROM schedules WHERE next_run<=?1 AND state!='running' ORDER BY next_run LIMIT 1",[now.timestamp()],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?))).optional()?;
    let Some((id, schedule, prompt)) = due else {
        return Ok(None);
    };
    let next = next_run(&schedule, now)?;
    let run_id = uuid::Uuid::new_v4().to_string();
    tx.execute("UPDATE schedules SET state='running',last_run=?2,next_run=?3,output=NULL,error=NULL WHERE id=?1",params![id,now.timestamp(),next])?;
    tx.execute(
        "INSERT INTO schedule_runs(id,schedule_id,started_at,state) VALUES(?1,?2,?3,'running')",
        params![run_id, id, now.timestamp()],
    )?;
    tx.commit()?;
    Ok(Some(ClaimedPrompt {
        id,
        prompt,
        run_id,
        cwd: cwd.into(),
        finished: false,
    }))
}

/// Called only while holding the exclusive project host lock.
pub fn recover_interrupted(cwd: &str) -> Result<()> {
    let mut db = connection(cwd)?;
    let tx = db.transaction()?;
    tx.execute("UPDATE schedules SET state='failed',error='Previous host exited during execution' WHERE state='running'",[])?;
    tx.execute("UPDATE schedule_runs SET state='failed',finished_at=?1,error='Previous host exited during execution' WHERE state='running'",[chrono::Utc::now().timestamp()])?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schedules_claim_once_and_cancellation_is_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_str().unwrap();
        let task = execute(
            json!({"operation":"create","schedule":"* * * * *","prompt":"Summarize"}),
            cwd,
        )
        .unwrap();
        let id = task["cron_id"].as_str().unwrap();
        connection(cwd)
            .unwrap()
            .execute("UPDATE schedules SET next_run=0", [])
            .unwrap();
        let claim = claim_due(cwd).unwrap().unwrap();
        assert!(claim_due(cwd).unwrap().is_none());
        assert!(execute(json!({"operation":"delete","cron_id":id}), cwd).is_err());
        drop(claim);
        assert_eq!(
            execute(json!({"operation":"get","cron_id":id}), cwd).unwrap()["state"],
            "failed"
        );
        connection(cwd)
            .unwrap()
            .execute("UPDATE schedules SET next_run=0", [])
            .unwrap();
        claim_due(cwd)
            .unwrap()
            .unwrap()
            .finish(Some("real output"), None)
            .unwrap();
        assert_eq!(
            execute(json!({"operation":"get","cron_id":id}), cwd).unwrap()["output"],
            "real output"
        );
    }
}
