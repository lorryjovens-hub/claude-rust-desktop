use crate::{args::PluginCommands, services};
use anyhow::{ensure, Result};
use claude_core::{runtime::RuntimePaths, services::plugins};
pub async fn run(paths: &RuntimePaths, action: &PluginCommands) -> Result<()> {
    let context = services::context(paths)?;
    match action {
        PluginCommands::ImportLegacy { source, preview } => services::print(
            &plugins::import_legacy(&context, std::path::Path::new(source), *preview)?,
        ),
        PluginCommands::List => services::print(&plugins::list(&context)?),
        PluginCommands::Install { source } => {
            services::print(&plugins::install(&context, source, None).await?)
        }
        PluginCommands::Update { id, source } => {
            services::print(&plugins::install(&context, source, Some(id)).await?)
        }
        PluginCommands::Enable { id } => {
            plugins::enable(&context, id, true)?;
            services::print(&serde_json::json!({"enabled":true}))
        }
        PluginCommands::Disable { id } => {
            plugins::enable(&context, id, false)?;
            services::print(&serde_json::json!({"enabled":false}))
        }
        PluginCommands::Search { query, registry } => {
            services::print(&plugins::search(registry, query).await?)
        }
        PluginCommands::Remove { id } => {
            plugins::remove(&context, id)?;
            services::print(&serde_json::json!({"removed":id}))
        }
        PluginCommands::Run {
            id,
            command,
            input,
            yes,
        } => {
            ensure!(
                *yes,
                "Plugin execution requires --yes; code runs with your OS permissions"
            );
            let input = serde_json::from_str(input)?;
            let root = std::env::current_dir()?;
            tokio::select! {result=plugins::run(&context,id,command,input,&root)=>services::print(&result?),_=tokio::signal::ctrl_c()=>anyhow::bail!("Plugin cancelled")}
        }
    }
}
