use crate::{
    args::{Cli, MagicCommands, PermissionModeArg},
    services::print,
};
use anyhow::{ensure, Result};
use claude_core::{
    runtime::RuntimePaths,
    services::{generation::Generator, lifecycle::Lease, magic_docs as docs},
};

pub async fn run(cli: &Cli, paths: &RuntimePaths, action: &MagicCommands) -> Result<()> {
    let context = crate::services::context(paths)?;
    let root = std::env::current_dir()?;
    match action {
        MagicCommands::ContextConfig { project_id, enabled } => {
            claude_core::services::magic_context::configure(&context, &root, project_id, *enabled)?;
            print(&serde_json::json!({"include_conversations":enabled}))
        }
        MagicCommands::ImportLegacy { source, preview } => {
            print(&docs::import_legacy(&context, &root, source, *preview)?)
        }
        MagicCommands::List => print(&docs::list(&context, &root)?),
        MagicCommands::Check { file } => print(&docs::check(&context, &root, file)?),
        MagicCommands::Previews => print(&docs::previews(&context, &root)?),
        MagicCommands::Clear => {
            print(&serde_json::json!({"untracked":docs::clear(&context, &root)?}))
        }
        MagicCommands::Auto { id, enabled } => {
            docs::set_auto(&context, &root, id, *enabled)?;
            print(&serde_json::json!({"auto_update":enabled}))
        }
        MagicCommands::Apply { id, yes } => {
            ensure!(*yes, "Inspect magic-docs previews then pass --yes to apply");
            let permissions = claude_core::runtime::default_permissions();
            permissions.set_mode(if matches!(cli.permission_mode, PermissionModeArg::Plan) {
                claude_core::permissions::PermissionMode::PlanMode
            } else {
                claude_core::permissions::PermissionMode::AcceptEdits
            });
            docs::apply(&context, &root, id, &permissions)?;
            print(&serde_json::json!({"applied":id}))
        }
        MagicCommands::Update {
            id,
            context: new_context,
        } => {
            let selection = crate::config::select(cli, paths, None, None)?;
            let generator = Generator::new(
                paths,
                selection.providers,
                &selection.provider_id,
                &selection.model,
            )?;
            tokio::select! { result=docs::update(&context,&root,id,new_context,&generator)=>print(&result?), _=tokio::signal::ctrl_c()=>anyhow::bail!("Update cancelled") }
        }
        MagicCommands::Start { interval } => {
            let selection = crate::config::select(cli, paths, None, None)?;
            let generator = Generator::new(
                paths,
                selection.providers,
                &selection.provider_id,
                &selection.model,
            )?;
            let lease = Lease::acquire(&context, "magic-docs")?;
            tokio::select! { result=docs::host(context,root,generator,*interval,None,lease)=>result, _=tokio::signal::ctrl_c()=>Ok(()) }
        }
    }
}
