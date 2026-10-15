//! Persistent experience collection and reviewable skill suggestions.
//! Suggestions are data, never an alternate tool executor or implicit permission.
use crate::services::ServiceContext;
use anyhow::{ensure, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Experience {
    pub id: String,
    pub session_id: String,
    pub context: String,
    pub intent: String,
    pub tools_used: Vec<String>,
    pub success: bool,
    pub duration_ms: u64,
    #[serde(default)]
    pub feedback: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Suggestion {
    pub id: String,
    pub intent: String,
    pub tools: Vec<String>,
    pub occurrences: usize,
    pub success_rate: f64,
    pub score: f64,
    pub average_duration_ms: u64,
    pub accepted: bool,
    pub skill_markdown: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LearningStatus {
    pub experiences: Vec<Experience>,
    pub suggestions: Vec<Suggestion>,
}

fn skill_slug(id: &str) -> String {
    // Stable bounded filename; collisions are rejected by exact content comparison.
    let hash = id.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ byte as u64).wrapping_mul(0x100000001b3)
    });
    format!("learned-{hash:016x}")
}

fn key(id: &str) -> Result<String> {
    ensure!(
        !id.is_empty() && id.len() <= 256,
        "Experience ID must contain 1–256 bytes"
    );
    Ok(format!("learning.experience.{id}"))
}

pub fn record(ctx: &ServiceContext, experience: &Experience) -> Result<()> {
    ensure!(
        experience.context.len() <= 32_768
            && experience.intent.len() <= 1024
            && experience.tools_used.len() <= 256,
        "Experience exceeds learning limits"
    );
    ensure!(
        experience
            .feedback
            .is_none_or(|r| r.is_finite() && (0.0..=1.0).contains(&r)),
        "Feedback must be between zero and one"
    );
    let key = key(&experience.id)?;
    let json = serde_json::to_string(experience)?;
    ctx.db.with_conn(|c| -> Result<()> {
        let tx = c.unchecked_transaction()?;
        let old: Option<String> = tx
            .query_row(
                "SELECT value FROM peripheral_settings WHERE key=?1",
                [&key],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            ensure!(
                serde_json::from_str::<Experience>(&old)? == *experience,
                "Experience ID conflicts with existing data"
            );
        } else {
            tx.execute(
                "INSERT INTO peripheral_settings(key,value) VALUES(?1,?2)",
                [&key, &json],
            )?;
        }
        tx.execute("DELETE FROM peripheral_settings WHERE key IN (SELECT key FROM peripheral_settings WHERE key LIKE 'learning.experience.%' ORDER BY rowid DESC LIMIT -1 OFFSET 1000)", [])?;
        tx.commit()?;
        Ok(())
    })??;
    Ok(())
}

pub fn feedback(ctx: &ServiceContext, id: &str, rating: f64) -> Result<()> {
    ensure!(
        rating.is_finite() && (0.0..=1.0).contains(&rating),
        "Feedback must be between zero and one"
    );
    let key = key(id)?;
    ctx.db.with_conn(|c| -> Result<()> {
        let tx = c.unchecked_transaction()?;
        let json: String = tx.query_row(
            "SELECT value FROM peripheral_settings WHERE key=?1",
            [&key],
            |r| r.get(0),
        )?;
        let mut experience: Experience = serde_json::from_str(&json)?;
        experience.feedback = Some(rating);
        tx.execute(
            "UPDATE peripheral_settings SET value=?2 WHERE key=?1",
            [&key, &serde_json::to_string(&experience)?],
        )?;
        tx.commit()?;
        Ok(())
    })??;
    Ok(())
}

pub fn status(ctx: &ServiceContext) -> Result<LearningStatus> {
    let experiences = ctx.db.with_conn(|c| -> Result<Vec<Experience>> {
        let mut statement = c.prepare("SELECT value FROM peripheral_settings WHERE key LIKE 'learning.experience.%' ORDER BY rowid")?;
        let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    })??;
    let mut groups: BTreeMap<(String, Vec<String>), Vec<&Experience>> = BTreeMap::new();
    for experience in &experiences {
        if !experience.tools_used.is_empty() {
            groups
                .entry((experience.intent.clone(), experience.tools_used.clone()))
                .or_default()
                .push(experience);
        }
        // Explicitly supplied experience context supports the old command/file patterns.
        // Automatic observations intentionally never copy prompt or command contents.
        let words: Vec<_> = experience.context.split_whitespace().collect();
        let mut observed = std::collections::BTreeSet::new();
        for pair in words.windows(2) {
            if ["bash", "shell", "exec", "run", "execute", "cmd"]
                .contains(&pair[0].to_lowercase().as_str())
            {
                observed.insert(format!("command: {}", pair[1]));
            }
        }
        if words.iter().any(|w| {
            ["read", "write", "edit", "create", "delete", "modify"]
                .contains(&w.to_lowercase().as_str())
        }) {
            for word in &words {
                if let Some(ext) = std::path::Path::new(word)
                    .extension()
                    .and_then(|s| s.to_str())
                {
                    if ["rs", "py", "js", "ts", "go", "md"].contains(&ext) {
                        observed.insert(format!("file operations: .{ext}"));
                    }
                }
            }
        }
        for intent in observed {
            groups
                .entry((intent, experience.tools_used.clone()))
                .or_default()
                .push(experience);
        }
    }
    let mut suggestions = Vec::new();
    for ((intent, tools), entries) in groups {
        if entries.len() < 3 {
            continue;
        }
        // Stable identity and recomputation prevent repeated extraction inflating evidence.
        let identity = serde_json::to_vec(&(&intent, &tools))?;
        use base64::Engine;
        let id = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(identity);
        let success_rate =
            entries.iter().filter(|e| e.success).count() as f64 / entries.len() as f64;
        let score = entries
            .iter()
            .map(|e| e.feedback.unwrap_or(if e.success { 1.0 } else { 0.0 }))
            .sum::<f64>()
            / entries.len() as f64;
        let accepted = ctx
            .setting::<bool>(&format!("learning.accepted.{id}"))?
            .unwrap_or(false);
        let skill_markdown = format!("---\nname: {}\ndescription: Reviewed workflow from local experience\n---\n\n# Observed workflow\n\nIntent: {}\n\nObserved tool sequence: {}\n\nValidate these observations for the current task. Use the normal tool permission checks.\n", skill_slug(&id), intent.replace('\n', " "), tools.join(" → "));
        let accepted = accepted
            && std::fs::read_to_string(
                ctx.paths
                    .data_dir
                    .join("skills")
                    .join(skill_slug(&id))
                    .join("SKILL.md"),
            )
            .ok()
            .as_deref()
                == Some(skill_markdown.as_str());
        suggestions.push(Suggestion {
            id,
            intent,
            tools,
            occurrences: entries.len(),
            success_rate,
            score,
            average_duration_ms: (entries.iter().map(|e| e.duration_ms as u128).sum::<u128>()
                / entries.len() as u128) as u64,
            accepted,
            skill_markdown,
        });
    }
    suggestions.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    Ok(LearningStatus {
        experiences,
        suggestions,
    })
}

pub fn accept(ctx: &ServiceContext, id: &str) -> Result<Suggestion> {
    let mut suggestion = status(ctx)?
        .suggestions
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| anyhow::anyhow!("Unknown learning suggestion"))?;
    let root = ctx.paths.data_dir.join("skills");
    std::fs::create_dir_all(&root)?;
    let root = root.canonicalize()?;
    let directory = root.join(skill_slug(id));
    match std::fs::create_dir(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    ensure!(
        !std::fs::symlink_metadata(&directory)?
            .file_type()
            .is_symlink()
            && directory.canonicalize()?.starts_with(&root),
        "Skill directory must remain inside the configured skills root"
    );
    let path = directory.join("SKILL.md");
    let verify = || -> Result<()> {
        ensure!(
            !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "Refusing to follow a skill file symlink"
        );
        ensure!(
            std::fs::read_to_string(&path)? == suggestion.skill_markdown,
            "An installed skill has different content; refusing to overwrite it"
        );
        Ok(())
    };
    if path.exists() {
        verify()?;
    } else {
        use std::io::Write;
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        temporary.write_all(suggestion.skill_markdown.as_bytes())?;
        temporary.as_file().sync_all()?;
        if let Err(error) = temporary.persist_noclobber(&path) {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                verify()?;
            } else {
                return Err(error.error.into());
            }
        }
    }
    ctx.save_setting(&format!("learning.accepted.{id}"), &true)?;
    suggestion.accepted = true;
    Ok(suggestion)
}

/// Drop records aborted tasks too. Only tool names and outcome are retained.
pub struct TurnObservation {
    pub tools: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ctx: ServiceContext,
    experience: Experience,
    started: std::time::Instant,
}
impl TurnObservation {
    pub fn new(ctx: ServiceContext, session_id: String) -> Self {
        Self {
            tools: Default::default(),
            ctx,
            experience: Experience {
                id: uuid::Uuid::new_v4().to_string(),
                session_id,
                context: String::new(),
                intent: "tool workflow".into(),
                tools_used: Vec::new(),
                success: false,
                duration_ms: 0,
                feedback: None,
            },
            started: std::time::Instant::now(),
        }
    }
    pub fn complete(&mut self, success: bool) {
        self.experience.success = success;
    }
}
impl Drop for TurnObservation {
    fn drop(&mut self) {
        self.experience.duration_ms =
            self.started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        if let Ok(tools) = self.tools.lock() {
            self.experience.tools_used = tools.iter().take(256).cloned().collect();
        }
        if record(&self.ctx, &self.experience).is_err() {
            tracing::warn!("Could not persist learning observation");
        }
    }
}
