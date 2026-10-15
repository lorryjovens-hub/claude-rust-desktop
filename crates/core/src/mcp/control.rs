//! Requests are delivered to an existing manager, never a temporary replacement process.
use super::McpServerManager;
use crate::services::{EnvironmentCredentials, ServiceContext};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
#[derive(Debug, Serialize)]
pub struct HostStatus {
    pub id: String,
    pub pid: u32,
    pub servers: Vec<String>,
}
pub struct Host {
    task: Option<tokio::task::JoinHandle<()>>,
    context: ServiceContext,
    id: String,
}
impl Host {
    pub(super) fn start(manager: McpServerManager) -> Result<Self> {
        let root = manager
            .config_path
            .parent()
            .context("MCP config has no parent")?;
        let context = ServiceContext::open(
            crate::runtime::RuntimePaths::new(root.to_path_buf())?,
            Arc::new(EnvironmentCredentials),
        )?;
        let id = uuid::Uuid::new_v4().to_string();
        let worker_context = context.clone();
        let worker_id = id.clone();
        let task = tokio::spawn(async move {
            if let Err(error) = serve(&worker_context, &worker_id, &manager).await {
                tracing::error!("MCP control host failed: {error}");
            }
        });
        Ok(Self {
            task: Some(task),
            context,
            id,
        })
    }
    pub async fn stop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        let _ = self
            .context
            .db
            .with_conn(|c| c.execute("DELETE FROM mcp_hosts WHERE id=?1", [&self.id]));
    }
}
async fn serve(context: &ServiceContext, id: &str, manager: &McpServerManager) -> Result<()> {
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    loop {
        tick.tick().await;
        let servers: Vec<_> = manager
            .list_servers()
            .await
            .into_iter()
            .filter(|s| s.running)
            .map(|s| s.id)
            .collect();
        let servers_json = serde_json::to_string(&servers)?;
        context.db.with_conn(|c|c.execute("INSERT INTO mcp_hosts(id,pid,expires_at,servers) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET expires_at=excluded.expires_at,servers=excluded.servers",params![id,std::process::id(),now()+30,servers_json]))??;
        let request:Option<(String,String)>=context.db.with_conn(|c|c.query_row("SELECT id,server FROM mcp_requests WHERE host=?1 AND state='pending' AND expires_at>?2 ORDER BY rowid LIMIT 1",params![id,now()],|r|Ok((r.get(0)?,r.get(1)?))).optional())??;
        if let Some((request, server)) = request {
            let claimed=context.db.with_conn(|c|c.execute("UPDATE mcp_requests SET state='running' WHERE id=?1 AND state='pending' AND expires_at>?2",params![request,now()]))??;
            if claimed != 1 {
                continue;
            }
            let result = if servers.contains(&server) {
                match tokio::time::timeout(Duration::from_secs(20), manager.restart_server(&server))
                    .await
                {
                    Ok(result) => result,
                    Err(_) => Err(anyhow::anyhow!("MCP restart timed out")),
                }
            } else {
                Err(anyhow::anyhow!("Server is no longer running in this host"))
            };
            let error = result.err().map(|e| e.to_string());
            context.db.with_conn(|c| {
                c.execute(
                    "UPDATE mcp_requests SET state=?1,error=?2 WHERE id=?3",
                    params![
                        if error.is_some() {
                            "failed"
                        } else {
                            "completed"
                        },
                        error,
                        request
                    ],
                )
            })??;
        }
    }
}
pub fn hosts(context: &ServiceContext) -> Result<Vec<HostStatus>> {
    context.db.with_conn(|c| -> Result<_> {
        let mut s =
            c.prepare("SELECT id,pid,servers FROM mcp_hosts WHERE expires_at>?1 ORDER BY id")?;
        let rows = s.query_map([now()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (id, pid, servers) = row?;
            Ok(HostStatus {
                id,
                pid,
                servers: serde_json::from_str(&servers)?,
            })
        })
        .collect()
    })?
}
pub async fn restart(context: &ServiceContext, server: &str, host: Option<&str>) -> Result<String> {
    let matches: Vec<_> = hosts(context)?
        .into_iter()
        .filter(|h| host.is_none_or(|id| id == h.id) && h.servers.iter().any(|s| s == server))
        .collect();
    ensure!(!matches.is_empty(), "No running host owns this MCP server");
    ensure!(
        matches.len() == 1,
        "Multiple hosts own this server; select --host from mcp hosts"
    );
    let id = uuid::Uuid::new_v4().to_string();
    let host = &matches[0].id;
    context.db.with_conn(|c|c.execute("INSERT INTO mcp_requests(id,host,server,expires_at,state) VALUES(?1,?2,?3,?4,'pending')",params![id,host,server,now()+25]))??;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(27);
    loop {
        let (state, error): (String, Option<String>) = context.db.with_conn(|c| {
            c.query_row(
                "SELECT state,error FROM mcp_requests WHERE id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })??;
        if state == "completed" {
            return Ok(host.clone());
        }
        if state == "failed" {
            anyhow::bail!("MCP restart failed: {}", error.unwrap_or_default());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "MCP host did not acknowledge restart; check host status"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
