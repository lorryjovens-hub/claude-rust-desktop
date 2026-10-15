//! SQLite leases are shared across processes; network work never holds a DB lock.
use super::ServiceContext;
use anyhow::{ensure, Result};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::time::Duration;

const LEASE_SECONDS: i64 = 15;
fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Debug, Serialize)]
pub struct ServiceStatus {
    pub name: String,
    pub pid: u32,
    pub state: String,
    pub error: Option<String>,
    pub completed: u64,
}

pub struct Lease {
    context: ServiceContext,
    pub name: String,
    owner: String,
}
impl Lease {
    pub fn acquire(context: &ServiceContext, name: &str) -> Result<Self> {
        let owner = uuid::Uuid::new_v4().to_string();
        let count = context.db.with_conn(|c| c.execute(
            "INSERT INTO service_leases(name,owner,pid,expires_at,state) VALUES(?1,?2,?3,?4,'running')
            ON CONFLICT(name) DO UPDATE SET owner=excluded.owner,pid=excluded.pid,expires_at=excluded.expires_at,
            stop_requested=0,state='running',error=NULL WHERE service_leases.expires_at<=?5",
            params![name,owner,std::process::id(),now()+LEASE_SECONDS,now()]))??;
        ensure!(
            count == 1,
            "Service {name} is already running in another host"
        );
        Ok(Self {
            context: context.clone(),
            name: name.into(),
            owner,
        })
    }
    pub fn renew(&self) -> Result<bool> {
        let count = self.context.db.with_conn(|c| c.execute(
            "UPDATE service_leases SET expires_at=?1 WHERE name=?2 AND owner=?3 AND stop_requested=0 AND expires_at>?4",
            params![now()+LEASE_SECONDS,self.name,self.owner,now()]))??;
        Ok(count == 1)
    }
    pub fn record(&self, error: Option<&str>) -> Result<()> {
        self.context.db.with_conn(|c| {
            c.execute(
            "UPDATE service_leases SET error=?1,completed=completed+?2 WHERE name=?3 AND owner=?4",
            params![error,if error.is_none() {1} else {0},self.name,self.owner])
        })??;
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.context.db.with_conn(|c| c.execute(
            "UPDATE service_leases SET expires_at=0,state='stopped',stop_requested=0 WHERE name=?1 AND owner=?2",
            params![self.name,self.owner]));
    }
}

pub fn status(context: &ServiceContext) -> Result<Vec<ServiceStatus>> {
    context.db.with_conn(|c| -> Result<_> {
        let mut stmt = c.prepare("SELECT name,pid,state,error,completed,expires_at,stop_requested FROM service_leases ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            let state: String = r.get(2)?;
            let expiry: i64 = r.get(5)?;
            let stop: bool = r.get(6)?;
            Ok(ServiceStatus {name:r.get(0)?,pid:r.get(1)?,
                state:if expiry <= now() && state == "running" {"expired".into()} else if stop {"stopping".into()} else {state},
                error:r.get(3)?,completed:r.get(4)?})
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })?
}

pub fn stop(context: &ServiceContext, name: &str) -> Result<()> {
    let count = context.db.with_conn(|c| c.execute(
        "UPDATE service_leases SET stop_requested=1 WHERE name=?1 AND expires_at>?2 AND state='running'", params![name,now()]))??;
    ensure!(count == 1, "Service {name} is not running");
    Ok(())
}

pub async fn wait_stopped(context: &ServiceContext, name: &str) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let running: bool = context
            .db
            .with_conn(|c| {
                c.query_row(
                    "SELECT expires_at>?2 AND state='running' FROM service_leases WHERE name=?1",
                    params![name, now()],
                    |r| r.get(0),
                )
                .optional()
            })??
            .unwrap_or(false);
        if !running {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Timed out waiting for {name} to stop"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A foreground host. Dropping this future cancels the in-flight HTTP request and releases its lease.
pub async fn run_team_sync(context: ServiceContext, interval_seconds: u64) -> Result<()> {
    ensure!(
        (1..=86400).contains(&interval_seconds),
        "Interval must be 1..86400 seconds"
    );
    let config = super::team::configured(&context)?;
    context.credentials.get(&config.credential)?;
    let lease = Lease::acquire(&context, "team-sync")?;
    run_team_sync_with_lease(context, interval_seconds, lease).await
}
pub async fn run_team_sync_with_lease(
    context: ServiceContext,
    interval_seconds: u64,
    lease: Lease,
) -> Result<()> {
    let mut heartbeat = tokio::time::interval(Duration::from_millis(250));
    loop {
        let operation = super::team::sync(&context);
        tokio::pin!(operation);
        let result = loop {
            tokio::select! {
                result = &mut operation => break result,
                _ = heartbeat.tick() => if !lease.renew()? { return Ok(()); },
            }
        };
        lease.record(result.as_ref().err().map(|e| e.to_string()).as_deref())?;
        let delay = tokio::time::sleep(Duration::from_secs(interval_seconds));
        tokio::pin!(delay);
        loop {
            tokio::select! {
                _ = &mut delay => break,
                _ = heartbeat.tick() => if !lease.renew()? { return Ok(()); },
            }
        }
    }
}
