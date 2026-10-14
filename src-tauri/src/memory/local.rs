//! Durable local memory. No external service is required.
use super::{MemoryConfig, MemoryItem, MemoryStats};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::{collections::{HashMap, HashSet}, path::PathBuf};

#[derive(Clone)]
pub(super) struct LocalMemory { pub path: PathBuf }

impl LocalMemory {
    fn open(&self) -> Result<Connection> {
        if let Some(parent) = self.path.parent() { std::fs::create_dir_all(parent)?; }
        let conn = Connection::open(&self.path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY, content TEXT NOT NULL UNIQUE, importance REAL NOT NULL,
            created_at TEXT NOT NULL, metadata TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS memory_settings (id INTEGER PRIMARY KEY CHECK (id = 1), config TEXT NOT NULL);")?;
        Ok(conn)
    }

    pub fn config(&self) -> Result<Option<MemoryConfig>> {
        let value: Option<String> = self.open()?.query_row("SELECT config FROM memory_settings WHERE id = 1", [], |row| row.get(0)).optional()?;
        value.map(|json| serde_json::from_str(&json).map_err(Into::into)).transpose()
    }

    pub fn save_config(&self, config: &MemoryConfig) -> Result<()> {
        self.open()?.execute("INSERT INTO memory_settings (id, config) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET config = excluded.config", [serde_json::to_string(config)?])?;
        Ok(())
    }

    pub fn ingest(&self, content: &str, importance: f64, metadata: Option<HashMap<String, String>>) -> Result<()> {
        self.open()?.execute("INSERT INTO memories (id, content, importance, created_at, metadata) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(content) DO UPDATE SET importance = MAX(memories.importance, excluded.importance), metadata = excluded.metadata",
            params![uuid::Uuid::new_v4().to_string(), content, importance, chrono::Utc::now().to_rfc3339(), serde_json::to_string(&metadata)?])?;
        Ok(())
    }

    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<MemoryItem>> {
        let conn = self.open()?;
        let mut statement = conn.prepare("SELECT id, content, importance, created_at, metadata FROM memories ORDER BY created_at DESC")?;
        let rows = statement.query_map([], |row| Ok(MemoryItem {
            id: row.get(0)?, content: row.get(1)?, importance: row.get(2)?, created_at: row.get(3)?,
            metadata: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(), similarity_score: None,
        }))?;
        let query = query.trim().to_lowercase();
        let terms = search_terms(&query);
        let mut results = Vec::new();
        for row in rows {
            let mut item = row?;
            if !query.is_empty() {
                let content = item.content.to_lowercase();
                let matches = terms.iter().filter(|term| content.contains(term.as_str())).count();
                let score = if content.contains(&query) { 1.0 } else if terms.is_empty() { 0.0 } else { matches as f64 / terms.len() as f64 };
                if score == 0.0 { continue; }
                item.similarity_score = Some(score);
            }
            results.push(item);
        }
        if !query.is_empty() {
            results.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.importance.total_cmp(&a.importance)).then_with(|| b.created_at.cmp(&a.created_at)));
        }
        results.truncate(top_k);
        Ok(results)
    }

    pub fn stats(&self) -> Result<MemoryStats> {
        let (count, chars): (usize, usize) = self.open()?.query_row("SELECT COUNT(*), COALESCE(SUM(length(content)), 0) FROM memories", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(MemoryStats { total_memories: count, total_tokens_approx: (chars + 2) / 3, backend: "Local SQLite (keyword search)".into() })
    }

    pub fn clear(&self) -> Result<()> {
        self.open()?.execute("DELETE FROM memories", [])?;
        Ok(())
    }
}

// Latin words and CJK bigrams support both English and Chinese recall.
fn search_terms(query: &str) -> HashSet<String> {
    let stop_words = ["the", "a", "an", "is", "are", "i", "my", "what", "was", "do", "you", "to", "of", "and", "in", "for"];
    let mut terms = HashSet::new();
    for word in query.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.is_empty() || stop_words.contains(&word) { continue; }
        let chars: Vec<char> = word.chars().collect();
        if chars.iter().any(|c| ('\u{3400}'..='\u{9fff}').contains(c)) && chars.len() > 1 {
            for pair in chars.windows(2) { terms.insert(pair.iter().collect()); }
        } else { terms.insert(word.to_string()); }
    }
    terms
}
