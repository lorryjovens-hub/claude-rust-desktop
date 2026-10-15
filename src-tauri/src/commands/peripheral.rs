use anyhow::{ensure, Context, Result};
use claude_core::{
    runtime::RuntimePaths,
    services::{dream, generation::Generator, lifecycle, team, voice, Credentials, ServiceContext},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tauri::Manager;
use tokio::sync::watch;

struct DesktopCredentials(tauri::AppHandle);
impl Credentials for DesktopCredentials {
    fn get(&self, name: &str) -> Result<String> {
        if let Ok(value) = std::env::var(name) {
            if !value.trim().is_empty() {
                return Ok(value);
            }
        }
        crate::secure_storage::SecureKeyStore::new()
            .get_service_credential(&self.0, name)?
            .filter(|s| !s.trim().is_empty())
            .with_context(|| format!("Credential {name} is not configured"))
    }
}
fn context(app: &tauri::AppHandle) -> Result<ServiceContext> {
    Ok(ServiceContext {
        paths: RuntimePaths::new(app.path().app_data_dir()?)?,
        db: app
            .state::<Arc<claude_core::db::DbManager>>()
            .inner()
            .clone(),
        credentials: Arc::new(DesktopCredentials(app.clone())),
    })
}
#[derive(Default)]
pub struct PeripheralHost {
    task: Mutex<Option<tokio::task::JoinHandle<Result<()>>>>,
    memory_tasks: Mutex<HashMap<String, tokio::task::JoinHandle<Result<()>>>>,
    voice_jobs: Mutex<HashMap<String, watch::Sender<bool>>>,
}
impl Drop for PeripheralHost {
    fn drop(&mut self) {
        if let Ok(task) = self.task.get_mut() {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
        if let Ok(tasks) = self.memory_tasks.get_mut() { for (_, task) in tasks.drain() { task.abort(); } }
        if let Ok(jobs) = self.voice_jobs.get_mut() {
            for (_, sender) in jobs.drain() {
                let _ = sender.send(true);
            }
        }
    }
}
pub async fn shutdown(app: &tauri::AppHandle) {
    let host = app.state::<PeripheralHost>();
    let task = host.task.lock().unwrap().take();
    if let Some(task) = task {
        task.abort();
        let _ = task.await;
    }
    let tasks: Vec<_> = host.memory_tasks.lock().unwrap().drain().map(|(_, task)| task).collect();
    for task in &tasks { task.abort(); }
    for task in tasks { let _ = task.await; }
    for (_, sender) in host.voice_jobs.lock().unwrap().drain() {
        let _ = sender.send(true);
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    SkillImportLegacy { source: std::path::PathBuf, preview: bool },
    Cron { project_id: String, operation: Value },
    CronStart { project_id: String, provider: String, model: String },
    CronStop,
    LearningStatus,
    LearningConfigure { enabled: bool },
    LearningFeedback { id: String, rating: f64 },
    LearningAccept { id: String },
    PerformanceStatus,
    PerformanceConfigure { config: claude_core::performance::RouterConfig },
    ProjectTemplates,
    ProjectScaffold { parent: std::path::PathBuf, name: String, template: String, template_file: Option<std::path::PathBuf>, #[serde(default)] variables: std::collections::HashMap<String,String>, #[serde(default)] git: bool, #[serde(default)] vscode: bool, preview: bool, #[serde(default)] allow_commands: bool },
    NetworkCancel { id: String },
    RemoteRequest { id: String, request: claude_core::services::remote_access::RemoteRequest, confirmed: bool },
    SshOperation { id: String, config: claude_core::services::remote_access::SshConfig, operation: claude_core::services::remote_access::SshOperation, confirmed: bool },
    SessionImport {source:std::path::PathBuf,project_id:String,preview:bool},
    McpHosts,
    McpRestart {server:String,host:String},
    PluginImportLegacy { source: String, preview: bool },
    Plugins,
    PluginSearch {registry:String,query:String},
    PluginInstall { source:String, update:Option<String> },
    PluginEnable { id:String, enabled:bool },
    PluginRemove { id:String },
    PluginRun { id:String, command:String, input:Value, project_id:String, confirmed:bool },
    Diagnostics { concurrency:usize, iterations:usize },
    UpdateCheck { url:String },
    AgentImportLegacy { source: std::path::PathBuf, id: String, preview: bool },
    Agents,
    AgentImport { definition: claude_core::services::agents::Definition },
    AgentCreate { project_id:String, agent_type:String, provider:String, model:String },
    Magic { project_id: String, operation: MagicRequest },
    DreamStatus,
    DreamConfig { config: dream::DreamConfig },
    DreamStart { automatic: bool },
    DreamStop { automatic: bool },
    Status,
    VoiceConfig {
        config: voice::VoiceConfig,
    },
    TeamAuth {
        config: team::TeamConfig,
    },
    TeamSync,
    TeamContext {project_id:String,enabled:Option<bool>},
    TeamPut {
        id: Option<String>,
        title: String,
        content: String,
        tags: Vec<String>,
    },
    TeamDelete {
        id: String,
    },
    TeamResolve {
        id: String,
        keep_local: bool,
    },
    TeamImport {
        source: std::path::PathBuf,
        preview: bool,
    },
    Start {
        interval: u64,
    },
    Stop,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MagicRequest {
    ContextConfig { enabled: bool },
    ImportLegacy {source:std::path::PathBuf,preview:bool},
    List,
    Check { file: std::path::PathBuf },
    Update { id:String, new_context:String, provider:String, model:String },
    Apply { id:String },
    Clear,
    Auto { id:String, enabled:bool },
    Start { provider:String, model:String, interval:u64 },
    Stop { automatic:bool },
}
#[tauri::command]
pub async fn peripheral_command(
    app: tauri::AppHandle,
    host: tauri::State<'_, PeripheralHost>,
    request: Request,
) -> Result<Value, String> {
    async {
        let context=context(&app)?;
        match request {
            Request::SkillImportLegacy {source,preview} => Ok(json!(claude_core::skills::import::import_legacy(&source,&context.paths.data_dir.join("skills"),preview)?)),
            Request::Cron {project_id,operation} => {
                let project=context.db.with_conn(|c|claude_core::db::project_repo::require_trusted(c,&project_id))??;
                ensure!(!project.is_archived,"Select an active project");
                let root=project.workspace_path.context("Project has no workspace")?;
                claude_core::tools::scheduled::execute(operation,&root)
            }
            Request::CronStart {project_id,provider,model} => {
                let project=context.db.with_conn(|c|claude_core::db::project_repo::require_trusted(c,&project_id))??;
                ensure!(!project.is_archived,"Select an active project");
                let root=project.workspace_path.context("Project has no workspace")?;
                Generator::saved(&context.paths,&provider,&model)?;
                let mut tasks=host.memory_tasks.lock().unwrap();
                ensure!(tasks.get("cron").is_none_or(|t|t.is_finished()),"Cron host is still running");
                let lease=lifecycle::Lease::acquire(&context,"cron")?;
                tasks.insert("cron".into(),tokio::spawn(claude_core::services::scheduled_prompts::run_with_lease(context,root,provider,model,lease)));
                Ok(json!({"started":true}))
            }
            Request::CronStop => {
                lifecycle::stop(&context,"cron")?; lifecycle::wait_stopped(&context,"cron").await?;
                let task=host.memory_tasks.lock().unwrap().remove("cron");
                if let Some(task)=task {let _=task.await;}
                Ok(json!({"stopped":true}))
            }
            Request::LearningStatus => {let mut value=json!(claude_core::learning::status(&context)?);value["enabled"]=json!(context.setting::<bool>("learning.enabled")?.unwrap_or(false));Ok(value)},
            Request::LearningConfigure { enabled } => {context.save_setting("learning.enabled",&enabled)?;Ok(json!({"enabled":enabled}))},
            Request::LearningFeedback { id, rating } => {claude_core::learning::feedback(&context,&id,rating)?;Ok(json!({"updated":id}))},
            Request::LearningAccept { id } => Ok(json!(claude_core::learning::accept(&context,&id)?)),
            Request::PerformanceStatus => Ok(json!(context.setting::<claude_core::performance::RouterConfig>("performance.router")?.unwrap_or_default())),
            Request::PerformanceConfigure { config } => {claude_core::performance::configure_router(&context,&config)?;Ok(json!(config))},
            Request::ProjectTemplates => Ok(json!(claude_core::services::project_templates::builtin_templates())),
            Request::ProjectScaffold { parent,name,template,template_file,variables,git,vscode,preview,allow_commands } => {
                use claude_core::services::project_templates as projects;
                let template=if let Some(file)=template_file { projects::load_template(&file)? } else { projects::builtin_templates().into_iter().find(|t|t.name==template).context("Unknown project template")? };
                let plan=projects::preview(&parent,&name,&template,&variables,git,vscode)?;
                if preview {Ok(json!(plan))} else {Ok(json!({"created":projects::create(plan,allow_commands).await?}))}
            }
            Request::NetworkCancel { id } => { if let Some(sender)=host.voice_jobs.lock().unwrap().remove(&id) { let _=sender.send(true); } Ok(json!({"cancelled":id})) },
            Request::RemoteRequest { id,request,confirmed } => {
                ensure!(confirmed || matches!(request.method.to_ascii_uppercase().as_str(),"GET"|"HEAD"),"Confirm the mutating HTTP request first");
                run_network(&host,id,async {Ok(json!(claude_core::services::remote_access::request(request).await?))}).await
            }
            Request::SshOperation { id,config,operation,confirmed } => {
                ensure!(confirmed || matches!(operation,claude_core::services::remote_access::SshOperation::Check),"Confirm the SSH operation first");
                run_network(&host,id,async {Ok(json!(claude_core::services::remote_access::ssh(&config,operation).await?))}).await
            }
            Request::SessionImport {source,project_id,preview} => {
                let project=context.db.with_conn(|c|claude_core::db::project_repo::require_trusted(c,&project_id))??;
                ensure!(!project.is_archived,"Select an active project");let root=project.workspace_path.context("Project has no workspace")?;
                Ok(json!(claude_core::services::session_import::import_into_project(&context,&source,std::path::Path::new(&root),preview,Some(&project_id))?))
            }

            Request::McpHosts => Ok(json!(claude_core::mcp::control::hosts(&context)?)),
            Request::McpRestart {server,host} => Ok(json!({"host":claude_core::mcp::control::restart(&context,&server,Some(&host)).await?})),
            Request::PluginSearch {registry,query} => Ok(json!(claude_core::services::plugins::search(&registry,&query).await?)),
            Request::PluginImportLegacy {source,preview} => Ok(json!(claude_core::services::plugins::import_legacy(&context,std::path::Path::new(&source),preview)?)),
            Request::Plugins => Ok(json!(claude_core::services::plugins::list(&context)?)),
            Request::PluginInstall {source,update} => Ok(json!(claude_core::services::plugins::install(&context,&source,update.as_deref()).await?)),
            Request::PluginEnable {id,enabled} => {claude_core::services::plugins::enable(&context,&id,enabled)?;Ok(json!({"enabled":enabled}))},
            Request::PluginRemove {id} => {claude_core::services::plugins::remove(&context,&id)?;Ok(json!({"removed":id}))},
            Request::PluginRun {id,command,input,project_id,confirmed} => {
                ensure!(confirmed,"Confirm plugin execution first");
                let project=context.db.with_conn(|c|claude_core::db::project_repo::require_trusted(c,&project_id))??;
                ensure!(!project.is_archived,"Select an active project");
                let root=project.workspace_path.context("Project has no workspace")?;
                claude_core::services::plugins::run(&context,&id,&command,input,std::path::Path::new(&root)).await
            },
            Request::Diagnostics {concurrency,iterations} => Ok(json!(claude_core::services::diagnostics::stress(concurrency,iterations,None,false).await?)),
            Request::UpdateCheck {url} => {let update=claude_core::services::updates::check(&url,env!("CARGO_PKG_VERSION")).await?;Ok(json!({"up_to_date":update.is_none(),"update":update}))}
            Request::AgentImportLegacy { source, id, preview } => Ok(json!(claude_core::services::agent_import::import(&context, &source, &id, preview)?)),
            Request::Agents => Ok(json!(claude_core::services::agents::list(&context)?)),
            Request::AgentImport { definition } => {claude_core::services::agents::install(&context,definition)?;Ok(json!({"imported":true}))}
            Request::AgentCreate { project_id,agent_type,provider,model } => {
                let project = context.db.with_conn(|c|claude_core::db::project_repo::require_trusted(c,&project_id))??;
                ensure!(!project.is_archived,"Select an active project");
                let root = project.workspace_path.context("Project has no workspace")?;
                let definition = claude_core::services::agents::resolve(&context,&agent_type)?;
                Generator::saved(&context.paths,&provider,&model)?;
                let store = claude_core::runtime::SessionStore::new(context.db.clone());
                let id = store.create(std::path::Path::new(&root),&model,&provider)?;
                // Preserve the exact project selected by the user, even if roots overlap.
                context.db.with_conn(|c|c.execute("UPDATE conversations SET project_id=?1,title=?2 WHERE id=?3",rusqlite::params![project_id,format!("{} agent",definition.id),id]))??;
                claude_core::services::agents::bind(&context.db,&id,&definition)?;
                Ok(json!({"id":id,"agent":definition.id}))
            }
            Request::Magic { project_id, operation } => {
                use claude_core::services::magic_docs as docs;
                let project = context.db.with_conn(|c| claude_core::db::project_repo::require_trusted(c, &project_id))??;
                ensure!(!project.is_archived, "Select an active project");
                let root = std::path::PathBuf::from(project.workspace_path.context("Project has no workspace")?);
                match operation {
                    MagicRequest::ContextConfig { enabled } => {claude_core::services::magic_context::configure(&context,&root,&project_id,enabled)?;Ok(json!({"include_conversations":enabled}))},
                    MagicRequest::ImportLegacy {source,preview}=>Ok(json!(docs::import_legacy(&context,&root,&source,preview)?)),
                    MagicRequest::List => Ok(json!({"include_conversations":claude_core::services::magic_context::enabled(&context,&project_id)?,"documents":docs::list(&context,&root)?, "previews":docs::previews(&context,&root)?, "services":lifecycle::status(&context)?})),
                    MagicRequest::Check { file } => Ok(json!(docs::check(&context,&root,&file)?)),
                    MagicRequest::Update { id,new_context,provider,model } => {
                        let generator = Generator::saved(&context.paths,&provider,&model)?;
                        Ok(json!(docs::update(&context,&root,&id,&new_context,&generator).await?))
                    }
                    MagicRequest::Apply { id } => {
                        let permissions = claude_core::runtime::default_permissions();
                        permissions.set_mode(claude_core::permissions::PermissionMode::AcceptEdits);
                        docs::apply(&context,&root,&id,&permissions)?; Ok(json!({"applied":id}))
                    }
                    MagicRequest::Clear => Ok(json!({"untracked":docs::clear(&context,&root)?})),
                    MagicRequest::Auto { id,enabled } => {docs::set_auto(&context,&root,&id,enabled)?; Ok(json!({"auto_update":enabled}))}
                    MagicRequest::Start { provider,model,interval } => {
                        ensure!((1..=86400).contains(&interval), "Interval must be 1..86400 seconds");
                        let generator = Generator::saved(&context.paths,&provider,&model)?;
                        let mut tasks = host.memory_tasks.lock().unwrap();
                        ensure!(tasks.get("magic-docs").is_none_or(|t|t.is_finished()), "Magic Docs is still running");
                        let lease = lifecycle::Lease::acquire(&context,"magic-docs")?;
                        tasks.insert("magic-docs".into(), tokio::spawn(docs::host(context,root,generator,interval,Some(project_id),lease)));
                        Ok(json!({"started":true}))
                    }
                    MagicRequest::Stop { automatic } => {
                        let name = if automatic {"magic-docs"} else {"magic-docs-update"};
                        lifecycle::stop(&context,name)?; lifecycle::wait_stopped(&context,name).await?;
                        let task = host.memory_tasks.lock().unwrap().remove(name);
                        if let Some(task) = task { let _ = task.await; }
                        Ok(json!({"stopped":true}))
                    }
                }
            }
            Request::DreamStatus => Ok(json!({"config":context.setting::<dream::DreamConfig>("auto-dream")?.unwrap_or_default(), "memory":dream::client(&context).dream_status().await?, "services":lifecycle::status(&context)?})),
            Request::DreamConfig { config } => {
                config.validate()?;
                Generator::saved(&context.paths, &config.provider, &config.model)?;
                context.save_setting("auto-dream", &config)?;
                Ok(json!(config))
            }
            Request::DreamStart { automatic } => {
                let config: dream::DreamConfig = context.setting("auto-dream")?.unwrap_or_default();
                config.validate()?;
                ensure!(!automatic || config.enabled, "Enable AutoDream first");
                let generator = Generator::saved(&context.paths, &config.provider, &config.model)?;
                // Validate storage before accepting the task.
                dream::client(&context).dream_status().await?;
                let name = if automatic { "auto-dream" } else { "memory-dream" };
                let mut tasks = host.memory_tasks.lock().unwrap();
                ensure!(tasks.get(name).is_none_or(|t| t.is_finished()), "Task is still running");
                let lease = lifecycle::Lease::acquire(&context, name)?;
                tasks.insert(name.into(), tokio::spawn(async move {
                    if automatic { dream::host(context, 60, lease).await }
                    else { dream::run_with_lease(&context, &generator, false, lease).await.map(|_| ()) }
                }));
                Ok(json!({"started":name}))
            }
            Request::DreamStop { automatic } => {
                let name = if automatic { "auto-dream" } else { "memory-dream" };
                lifecycle::stop(&context, name)?;
                lifecycle::wait_stopped(&context, name).await?;
                let task = host.memory_tasks.lock().unwrap().remove(name);
                if let Some(task) = task { let _ = task.await; }
                Ok(json!({"stopped":name}))
            }
            Request::Status=>Ok(json!({"voice":context.setting::<voice::VoiceConfig>("voice")?,
                "team":context.setting::<team::TeamConfig>("team-sync")?,"services":lifecycle::status(&context)?,
                "memories":if context.setting::<team::TeamConfig>("team-sync")?.is_some(){team::list(&context,false)?}else{vec![]},
                "conflicts":if context.setting::<team::TeamConfig>("team-sync")?.is_some(){team::conflicts(&context)?}else{vec![]}})),
            Request::VoiceConfig {config}=> {config.validate()?;context.save_setting("voice",&config)?;Ok(json!(config))}
            Request::TeamAuth {config}=> {team::auth(&context,config).await?;Ok(json!({"authenticated":true}))}
            Request::TeamContext {project_id,enabled} => {
                let project=context.db.with_conn(|c|claude_core::db::project_repo::require_trusted(c,&project_id))??;
                ensure!(!project.is_archived,"Select an active project");
                let root=project.workspace_path.context("Project has no workspace")?;
                if let Some(enabled)=enabled {Ok(json!(team::configure_model_context(&context,std::path::Path::new(&root),enabled)?))}
                else {Ok(json!(team::model_context_setting(&context,std::path::Path::new(&root))?))}
            }
            Request::TeamSync=>Ok(json!(team::sync(&context).await?)),
            Request::TeamPut {id,title,content,tags}=>Ok(json!(team::put(&context,id,title,content,tags)?)),
            Request::TeamDelete {id}=> {team::delete(&context,&id)?;Ok(json!({"deleted":id}))}
            Request::TeamResolve {id,keep_local}=> {team::resolve(&context,&id,keep_local)?;Ok(json!({"resolved":id}))}
            Request::TeamImport {source,preview}=>Ok(json!(team::import_legacy(&context,&source,preview)?)),
            Request::Start {interval}=> {
                ensure!((1..=86400).contains(&interval),"Interval must be 1..86400 seconds");
                team::configured(&context)?;
                context.credentials.get(&team::configured(&context)?.credential)?;
                let mut task=host.task.lock().unwrap();
                ensure!(task.as_ref().is_none_or(|t|t.is_finished()),"Desktop already hosts team-sync");
                // Acquire before returning so duplicate hosts cannot both report success.
                let lease=lifecycle::Lease::acquire(&context,"team-sync")?;
                *task=Some(tokio::spawn(lifecycle::run_team_sync_with_lease(context,interval,lease)));
                Ok(json!({"started":true}))
            }
            Request::Stop=> {lifecycle::stop(&context,"team-sync")?;lifecycle::wait_stopped(&context,"team-sync").await?;
                let task=host.task.lock().unwrap().take();if let Some(task)=task {task.await??;}
                Ok(json!({"stopped":true}))}
        }
    }.await.map_err(|e:anyhow::Error|e.to_string())
}
#[tauri::command]
pub fn peripheral_credential(
    app: tauri::AppHandle,
    name: String,
    value: String,
) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 128
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("Invalid credential name".into());
    }
    crate::secure_storage::SecureKeyStore::new()
        .set_service_credential(&app, &name, &value)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn peripheral_credential_delete(app: tauri::AppHandle, name: String) -> Result<(), String> {
    if !name.starts_with("STAGED_") || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Only staged peripheral credentials can be removed".into());
    }
    crate::secure_storage::SecureKeyStore::new().delete_service_credential(&app, &name).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn voice_begin(
    app: tauri::AppHandle,
    host: tauri::State<'_, PeripheralHost>,
) -> Result<String, String> {
    let prepare = || -> Result<_> {
        let context = context(&app)?;
        let config: voice::VoiceConfig = context
            .setting("voice")?
            .context("请先在设置 → 服务与团队中配置音频转写后端")?;
        config.validate()?;
        context.credentials.get(&config.credential)?;
        let id = uuid::Uuid::new_v4().to_string();
        host.voice_jobs
            .lock()
            .unwrap()
            .insert(id.clone(), watch::channel(false).0);
        Ok(id)
    };
    prepare().map_err(|e| e.to_string())
}
#[tauri::command]
pub fn voice_cancel(host: tauri::State<'_, PeripheralHost>, id: String) {
    if let Some(sender) = host.voice_jobs.lock().unwrap().remove(&id) {
        let _ = sender.send(true);
    }
}
#[tauri::command]
pub async fn voice_transcribe(
    app: tauri::AppHandle,
    host: tauri::State<'_, PeripheralHost>,
    id: String,
    audio: Vec<u8>,
    mime: String,
) -> Result<String, String> {
    let receiver = host
        .voice_jobs
        .lock()
        .unwrap()
        .get(&id)
        .map(|s| s.subscribe());
    let Some(mut cancellation) = receiver else {
        return Err("Recording was cancelled".into());
    };
    let result = async {
        let context = context(&app)?;
        tokio::select! {
            biased;
            _=cancellation.changed()=>Err(anyhow::anyhow!("Transcription was cancelled")),
            result=voice::transcribe(&context,audio,&mime)=>result,
        }
    }
    .await
    .map_err(|e: anyhow::Error| e.to_string());
    host.voice_jobs.lock().unwrap().remove(&id);
    result
}

async fn run_network(host: &PeripheralHost, id: String, operation: impl std::future::Future<Output=anyhow::Result<Value>>) -> anyhow::Result<Value> {
    let (sender, mut cancel) = watch::channel(false);
    {
        let mut jobs=host.voice_jobs.lock().unwrap();
        ensure!(!jobs.contains_key(&id), "Operation ID is already active");
        jobs.insert(id.clone(),sender);
    }
    struct Cleanup<'a> { host: &'a PeripheralHost, id: String }
    impl Drop for Cleanup<'_> { fn drop(&mut self) { self.host.voice_jobs.lock().unwrap().remove(&self.id); } }
    let _cleanup=Cleanup{host,id};
    tokio::select! {result=operation=>result, _=cancel.changed()=>anyhow::bail!("Remote operation cancelled")}
}
