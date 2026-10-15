use crate::args::MemoryCommands;
use anyhow::{ensure, Result};
use claude_core::{memory::MemExClient, runtime::RuntimePaths};

pub async fn run(cli: &crate::args::Cli, paths: &RuntimePaths, action: &MemoryCommands) -> Result<()> {
    let client = MemExClient::with_path(paths.data_dir.join("memory.sqlite3"), None);
    match action {
        MemoryCommands::Dream | MemoryCommands::AutoDream => {
            let selected = crate::config::select(cli, paths, None, None)?;
            let generator = claude_core::services::generation::Generator::new(paths, selected.providers, &selected.provider_id, &selected.model)?;
            let context = crate::services::context(paths)?;
            tokio::select! {
                result = claude_core::services::dream::run(&context, &generator, false) => crate::services::print(&result?),
                _ = tokio::signal::ctrl_c() => anyhow::bail!("Dream cancelled"),
            }
        }
        MemoryCommands::DreamStatus => {
            let context = crate::services::context(paths)?;
            crate::services::print(&serde_json::json!({"memory":client.dream_status().await?, "config":context.setting::<claude_core::services::dream::DreamConfig>("auto-dream")?.unwrap_or_default(), "services":claude_core::services::lifecycle::status(&context)?}))
        }
        MemoryCommands::DreamConfig { enabled, min_hours, min_memories } => {
            let selected = crate::config::select(cli, paths, None, None)?;
            let config = claude_core::services::dream::DreamConfig { enabled:*enabled, provider:selected.provider_id, model:selected.model, min_hours:*min_hours, min_memories:*min_memories };
            config.validate()?;
            claude_core::services::generation::Generator::saved(paths, &config.provider, &config.model)?;
            crate::services::context(paths)?.save_setting("auto-dream", &config)?;
            crate::services::print(&config)
        }
        MemoryCommands::Status => crate::services::print(&client.stats().await?),
        MemoryCommands::Clear { yes } => {
            ensure!(
                *yes,
                "Pass --yes to clear personal memory; conversations and team memory are retained"
            );
            client.clear().await?;
            crate::services::print(&serde_json::json!({"cleared": true}))
        }
        MemoryCommands::Export { output } => {
            let archive = client.export_archive().await?;
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            file.write_all(&serde_json::to_vec_pretty(&archive)?)?;
            file.sync_all()?;
            crate::services::print(
                &serde_json::json!({"exported": archive.memories.len(), "output": output}),
            )
        }
        MemoryCommands::Import { input, preview } => {
            let value = serde_json::from_slice(&std::fs::read(input)?)?;
            crate::services::print(&client.import_archive(value, *preview).await?)
        }
    }
}
