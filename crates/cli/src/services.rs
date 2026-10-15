use crate::args::{ServiceCommands, TeamCommands};
use anyhow::Result;
use claude_core::services::{lifecycle, team, team_server::TeamServer, ServiceContext};
use serde::Serialize;
pub fn context(paths: &claude_core::runtime::RuntimePaths) -> Result<ServiceContext> {
    ServiceContext::open(
        paths.clone(),
        std::sync::Arc::new(claude_core::services::EnvironmentCredentials),
    )
}
pub fn print(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
pub async fn team(context: &ServiceContext, action: &TeamCommands) -> Result<()> {
    match action {
        TeamCommands::Context {enabled}=>print(&team::configure_model_context(context,&std::env::current_dir()?,*enabled)?),
        TeamCommands::Status => print(&context.setting::<team::TeamConfig>("team-sync")?),
        TeamCommands::Auth {
            team_id,
            endpoint,
            credential,
        } => {
            team::auth(
                context,
                team::TeamConfig {
                    endpoint: endpoint.clone(),
                    team: team_id.clone(),
                    credential: credential.clone(),
                },
            )
            .await?;
            print(&serde_json::json!({"authenticated":true,"team":team_id}))
        }
        TeamCommands::Sync => print(&team::sync(context).await?),
        TeamCommands::List { include_deleted } => print(&team::list(context, *include_deleted)?),
        TeamCommands::Create {
            title,
            content,
            tags,
        }
        | TeamCommands::Update {
            title,
            content,
            tags,
            ..
        } => {
            let id = if let TeamCommands::Update { id, .. } = action {
                Some(id.clone())
            } else {
                None
            };
            print(&team::put(
                context,
                id,
                title.clone(),
                content.clone(),
                tags.as_deref()
                    .unwrap_or("")
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().into())
                    .collect(),
            )?)
        }
        TeamCommands::Delete { id } => {
            team::delete(context, id)?;
            print(&serde_json::json!({"deleted":id}))
        }
        TeamCommands::Conflicts => print(&team::conflicts(context)?),
        TeamCommands::Resolve { id, keep } => {
            team::resolve(context, id, keep == "local")?;
            print(&serde_json::json!({"resolved":id}))
        }
        TeamCommands::ImportLegacy { source, preview } => {
            print(&team::import_legacy(context, source, *preview)?)
        }
        TeamCommands::Serve { bind, tokens_env } => {
            let tokens = serde_json::from_str(&context.credentials.get(tokens_env)?)?;
            let router =
                TeamServer::new(context.paths.data_dir.join("team-server.sqlite3"), tokens)?
                    .router();
            let listener = tokio::net::TcpListener::bind(bind).await?;
            println!(
                "{}",
                serde_json::json!({"listening":listener.local_addr()?.to_string(),"protocol":1})
            );
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
            Ok(())
        }
    }
}
pub async fn services(context: &ServiceContext, action: &ServiceCommands) -> Result<()> {
    match action {
        ServiceCommands::Status => print(&lifecycle::status(context)?),
        ServiceCommands::Start { name, interval } => {
            eprintln!("Hosting {name}; Ctrl+C or services stop {name} stops this instance.");
            if name == "auto-dream" {
                let config: claude_core::services::dream::DreamConfig = context.setting("auto-dream")?.unwrap_or_default();
                config.validate()?;
                anyhow::ensure!(config.enabled, "Enable AutoDream with memory dream-config first");
                claude_core::services::generation::Generator::saved(&context.paths, &config.provider, &config.model)?;
                let lease = lifecycle::Lease::acquire(context, name)?;
                return tokio::select! { result=claude_core::services::dream::host(context.clone(),*interval,lease)=>result, _=tokio::signal::ctrl_c()=>Ok(()) };
            }
            tokio::select! {result=lifecycle::run_team_sync(context.clone(),*interval)=>result,_=tokio::signal::ctrl_c()=>Ok(())}
        }
        ServiceCommands::Stop { name } => {
            lifecycle::stop(context, name)?;
            lifecycle::wait_stopped(context, name).await?;
            print(&serde_json::json!({"stopped":name}))
        }
        ServiceCommands::TeamSync => print(&context.setting::<team::TeamConfig>("team-sync")?),
        ServiceCommands::Voice => {
            print(&context.setting::<claude_core::services::voice::VoiceConfig>("voice")?)
        }
    }
}
