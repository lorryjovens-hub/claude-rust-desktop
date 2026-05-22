pub mod schema;
pub mod conversation_repo;
pub mod message_repo;
pub mod project_repo;
pub mod migration;

use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct DbManager {
    conn: Mutex<Connection>,
}

impl DbManager {
    pub fn new(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn init(&self) -> Result<()> {
        Ok(self.with_conn(|conn| conn.execute_batch(schema::SCHEMA_SQL))??)
    }

    pub fn with_conn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Connection) -> R,
    {
        let guard = self.conn.lock().map_err(|e| anyhow::anyhow!("DB mutex poisoned: {}", e))?;
        Ok(f(&guard))
    }
}
