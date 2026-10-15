//! Minimal independently deployable protocol v1 server; bearer credentials are supplied by its operator.
use super::team::{validate_memory, Mutation, SyncRequest, SyncResponse, TeamMemory};
use anyhow::{ensure, Result};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

#[derive(Clone)]
pub struct TeamServer {
    path: PathBuf,
    tokens: Arc<HashMap<String, String>>,
}
impl TeamServer {
    /// tokens maps opaque bearer credentials to exactly one team. No credential is minted locally.
    pub fn new(path: PathBuf, tokens: HashMap<String, String>) -> Result<Self> {
        ensure!(
            !tokens.is_empty()
                && tokens
                    .iter()
                    .all(|(k, v)| k.len() >= 16 && !v.trim().is_empty()),
            "Supply at least one token (16+ characters) mapped to a team"
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let server = Self {
            path,
            tokens: Arc::new(tokens),
        };
        server.open()?.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS team_sequence(team TEXT PRIMARY KEY,revision INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS team_entries(team TEXT NOT NULL,id TEXT NOT NULL,revision INTEGER NOT NULL,memory TEXT NOT NULL,PRIMARY KEY(team,id));
            CREATE TABLE IF NOT EXISTS team_mutations(team TEXT NOT NULL,mutation TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(team,mutation));")?;
        Ok(server)
    }
    fn open(&self) -> Result<Connection> {
        let c = Connection::open(&self.path)?;
        c.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(c)
    }
    fn authorize(&self, headers: &HeaderMap, team: &str) -> Result<(), StatusCode> {
        let token = headers
            .get("authorization")
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        match self.tokens.get(token) {
            Some(allowed) if allowed == team => Ok(()),
            _ => Err(StatusCode::UNAUTHORIZED),
        }
    }
    pub fn router(self) -> Router {
        Router::new()
            .route("/v1/teams/{team}/auth", get(auth))
            .route("/v1/teams/{team}/sync", post(sync))
            .layer(axum::extract::DefaultBodyLimit::max(8 * 1024 * 1024))
            .with_state(self)
    }
    fn synchronize(&self, team: &str, request: SyncRequest) -> Result<SyncResponse> {
        ensure!(
            request.cursor >= 0 && request.changes.len() <= 1000,
            "Invalid synchronization batch"
        );
        let mut c = self.open()?;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut cursor: i64 = tx
            .query_row(
                "SELECT revision FROM team_sequence WHERE team=?1",
                [team],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        ensure!(
            request.cursor <= cursor,
            "Client cursor exceeds server history"
        );
        let mut accepted = Vec::new();
        let mut conflicts = Vec::new();
        for mutation in request.changes {
            validate_memory(&mutation.memory)?;
            ensure!(
                !mutation.mutation.is_empty() && mutation.mutation.len() <= 128,
                "Invalid mutation ID"
            );
            let previous: Option<String> = tx
                .query_row(
                    "SELECT result FROM team_mutations WHERE team=?1 AND mutation=?2",
                    params![team, mutation.mutation],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(previous) = previous {
                accepted.push(serde_json::from_str::<Mutation>(&previous)?);
                continue;
            }
            let existing: Option<String> = tx
                .query_row(
                    "SELECT memory FROM team_entries WHERE team=?1 AND id=?2",
                    params![team, mutation.memory.id],
                    |r| r.get(0),
                )
                .optional()?;
            let existing = existing
                .map(|s| serde_json::from_str::<TeamMemory>(&s))
                .transpose()?;
            if mutation.memory.revision != existing.as_ref().map(|m| m.revision).unwrap_or(0) {
                conflicts.push(
                    existing.ok_or_else(|| anyhow::anyhow!("Unknown revision for new memory"))?,
                );
                continue;
            }
            cursor += 1;
            let mut saved = mutation.clone();
            saved.memory.revision = cursor;
            tx.execute("INSERT INTO team_entries(team,id,revision,memory) VALUES(?1,?2,?3,?4) ON CONFLICT(team,id) DO UPDATE SET revision=excluded.revision,memory=excluded.memory",
                params![team,saved.memory.id,cursor,serde_json::to_string(&saved.memory)?])?;
            tx.execute(
                "INSERT INTO team_mutations(team,mutation,result) VALUES(?1,?2,?3)",
                params![team, saved.mutation, serde_json::to_string(&saved)?],
            )?;
            accepted.push(saved);
        }
        tx.execute("INSERT INTO team_sequence(team,revision) VALUES(?1,?2) ON CONFLICT(team) DO UPDATE SET revision=excluded.revision",params![team,cursor])?;
        let changes = {
            let mut stmt = tx.prepare(
                "SELECT memory FROM team_entries WHERE team=?1 AND revision>?2 ORDER BY revision",
            )?;
            let json = stmt
                .query_map(params![team, request.cursor], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            json.into_iter()
                .map(|s| serde_json::from_str(&s))
                .collect::<serde_json::Result<Vec<TeamMemory>>>()?
        };
        tx.commit()?;
        Ok(SyncResponse {
            cursor,
            changes,
            accepted,
            conflicts,
        })
    }
}
async fn auth(
    State(server): State<TeamServer>,
    Path(team): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    server.authorize(&headers, &team)?;
    Ok(Json(serde_json::json!({"team":team,"protocol":1})))
}
async fn sync(
    State(server): State<TeamServer>,
    Path(team): Path<String>,
    headers: HeaderMap,
    Json(request): Json<SyncRequest>,
) -> Result<Json<SyncResponse>, StatusCode> {
    server.authorize(&headers, &team)?;
    tokio::task::spawn_blocking(move || server.synchronize(&team, request))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .map_err(|_| StatusCode::BAD_REQUEST)
}
