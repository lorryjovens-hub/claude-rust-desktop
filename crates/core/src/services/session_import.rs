use super::ServiceContext;
use crate::db::{conversation_repo, message_repo, project_repo};
use anyhow::{ensure, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Deserialize)]
struct Session {
    id: String,
    name: String,
    created_at: String,
    updated_at: String,
    messages: Vec<Message>,
}
#[derive(Deserialize)]
struct Message {
    role: String,
    content: String,
    timestamp: String,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub imported: usize,
    pub skipped: usize,
    pub preview: bool,
    pub backup: Option<String>,
    pub project_id: String,
    pub sessions: Vec<String>,
}
pub fn import(
    context: &ServiceContext,
    source: &Path,
    workspace: &Path,
    preview: bool,
) -> Result<Report> {
    import_into_project(context, source, workspace, preview, None)
}
pub fn import_into_project(
    context: &ServiceContext,
    source: &Path,
    workspace: &Path,
    preview: bool,
    target_project: Option<&str>,
) -> Result<Report> {
    let root = std::fs::canonicalize(workspace)?;
    ensure!(root.is_dir(), "Project directory required");
    let source = std::fs::canonicalize(source)?;
    let mut files = if source.is_dir() {
        std::fs::read_dir(&source)?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect::<Vec<_>>()
    } else {
        vec![source]
    };
    files.sort();
    ensure!(
        !files.is_empty() && files.len() <= 1000,
        "Choose 1..1000 session JSON files"
    );
    let mut bytes = 0;
    let mut sessions = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for file in files {
        bytes += std::fs::metadata(&file)?.len();
        ensure!(bytes <= 25 * 1024 * 1024, "Session archive exceeds 25 MiB");
        let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&file)?)?;
        let session: Session = serde_json::from_value(raw.clone())?;
        ensure!(
            !session.id.is_empty() && session.id.len() <= 256 && ids.insert(session.id.clone()),
            "Empty, oversized or duplicate session ID"
        );
        chrono::DateTime::parse_from_rfc3339(&session.created_at)?;
        chrono::DateTime::parse_from_rfc3339(&session.updated_at)?;
        ensure!(
            session.messages.len() <= 10000,
            "Session exceeds 10000 messages"
        );
        for message in &session.messages {
            ensure!(
                matches!(message.role.as_str(), "user" | "assistant" | "system"),
                "Unsupported legacy message role {}; import requires complete text sessions",
                message.role
            );
            chrono::DateTime::parse_from_rfc3339(&message.timestamp)?;
        }
        sessions.push((file, session, raw.to_string()));
    }
    let backup = if preview {
        None
    } else {
        let directory = context.paths.data_dir.join("backups");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("sessions-{}.sqlite3", uuid::Uuid::new_v4()));
        context
            .db
            .with_conn(|c| c.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()]))??;
        Some(path.to_string_lossy().into_owned())
    };
    context.db.with_conn(|c|->Result<_>{
        let tx=c.unchecked_transaction()?;tx.execute("UPDATE peripheral_migrations SET version=version WHERE version=8",[])?;
        let project=project_repo::list_projects(&tx)?.into_iter().find(|p|target_project.is_none_or(|id|p.id==id)&&!p.is_archived&&p.workspace_path.as_deref().and_then(|p|std::fs::canonicalize(p).ok()).as_ref()==Some(&root)&&!p.folders.is_empty()&&p.folders.iter().all(|f|f.trusted));
        let project_id=if let Some(project)=project {project.id}else {
            ensure!(target_project.is_none(),"Selected project is unavailable or untrusted");
            let id=uuid::Uuid::new_v4().to_string();let now=chrono::Utc::now().to_rfc3339();let path=root.to_string_lossy().into_owned();
            project_repo::insert_project(&tx,&id,&root.file_name().unwrap_or_default().to_string_lossy(),None,None,Some(&path),false,&now,&now)?;
            project_repo::set_folders(&tx,&id,&[path.clone()])?;project_repo::trust_folders(&tx,&id,&[path])?;id
        };
        let mut report=Report{imported:0,skipped:0,preview,backup,project_id:project_id.clone(),sessions:Vec::new()};
        for(file,session,payload) in sessions {
            let prior:Option<(String,String)>=tx.query_row("SELECT payload,project_id FROM legacy_session_imports WHERE id=?1",[&session.id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((prior,project))=prior {ensure!(prior==payload&&project==project_id,"Legacy session {} conflicts with a previous import",session.id);ensure!(conversation_repo::get_conversation(&tx,&session.id)?.is_some(),"Previously imported session was deleted; choose a new source ID to restore it");report.skipped+=1;continue;}
            ensure!(conversation_repo::get_conversation(&tx,&session.id)?.is_none(),"Session ID {} already exists",session.id);
            conversation_repo::insert_conversation(&tx,&session.id,Some(&session.name),None,None,Some(&root.to_string_lossy()),Some(&project_id),false,false,false,&session.created_at,&session.updated_at,session.messages.len() as i64)?;
            conversation_repo::set_workspace_mode(&tx,&session.id,"code")?;
            for(index,message) in session.messages.iter().enumerate(){message_repo::insert_message(&tx,&format!("{}:legacy:{index}",session.id),&session.id,&message.role,&message.content,None,&message.timestamp,false,index as i64)?;}
            tx.execute("INSERT INTO legacy_session_imports(id,source,payload,project_id) VALUES(?1,?2,?3,?4)",params![session.id,file.to_string_lossy(),payload,project_id])?;
            report.imported+=1;report.sessions.push(session.id);
        }
        if preview {tx.rollback()?;}else{tx.commit()?;}Ok(report)
    })?
}
