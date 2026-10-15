mod interactive_config;
mod args;
mod migration;
mod config;
#[allow(dead_code)]
mod legacy_api_config;
mod session;
mod services;
mod memory;
mod magic_docs;
mod plugins;
mod help;
mod voice;
#[allow(dead_code)]
mod terminal;

use anyhow::{bail, ensure, Context, Result};
use args::{Cli, Commands, McpCommands, OutputFormat, SessionCommands, SkillsCommands};
use clap::Parser;
use claude_core::{
    mcp::{McpServerConfig, McpServerManager},
    runtime::{RuntimePaths, SessionStore},
    skills::SkillsManager,
};
use std::{
    collections::HashMap,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(if cli.verbose { "info" } else { "error" })
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .init();
    if let Err(error) = run(&cli).await {
        eprintln!("Error: {error:#}");
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

fn absolute(path: PathBuf) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
}

async fn run(cli: &Cli) -> Result<()> {
    if let Some(Commands::Help {topic,command})=&cli.command {return help::show(topic.as_deref(),command); }
    let data_dir = match &cli.data_dir {
        Some(path) => path.clone(),
        None => RuntimePaths::default_data_dir()?,
    };
    let paths = RuntimePaths::new(absolute(data_dir)?)?;
    if let Some(path) = &cli.path {
        std::env::set_current_dir(path).context("Cannot open project directory")?;
    }
    if cli.info {
        println!("claude-code {}\nOS: {}\nProject: {}\nData: {}\nEngine: claude-core (direct; no Bridge)", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::current_dir()?.display(), paths.data_dir.display());
        return Ok(());
    }
    let mut skill_prompt = None;
    match &cli.command {
        Some(Commands::Plugin { action }) => return plugins::run(&paths,action).await,
        Some(Commands::Update { url }) => {
            let update = claude_core::services::updates::check(url,env!("CARGO_PKG_VERSION")).await?;
            return services::print(&serde_json::json!({"up_to_date":update.is_none(),"update":update}));
        }
        Some(Commands::StressTest { concurrency, iterations, endpoint, allow_remote }) => {
            return tokio::select! {result=claude_core::services::diagnostics::stress(*concurrency,*iterations,endpoint.clone(),*allow_remote)=>services::print(&result?), _=tokio::signal::ctrl_c()=>anyhow::bail!("Diagnostics cancelled")};
        }
        Some(Commands::Learning { action }) => return migration::learning(&paths, action).await,
        Some(Commands::Performance { action }) => return migration::performance(&paths, action).await,
        Some(Commands::Project { action }) => return migration::project(action).await,
        Some(Commands::Cron { action }) => return migration::cron(&paths, &std::env::current_dir()?, action).await,
        Some(Commands::Ssh { config, operation, yes }) => return tokio::select! { result=migration::ssh(config,operation,*yes)=>result, _=tokio::signal::ctrl_c()=>anyhow::bail!("SSH operation cancelled") },
        Some(Commands::Remote { request, yes }) => return tokio::select! { result=migration::remote(request,*yes)=>result, _=tokio::signal::ctrl_c()=>anyhow::bail!("Remote operation cancelled") },
        Some(Commands::Agents { action }) => {
            let context = services::context(&paths)?;
            match action {
                args::AgentCommands::ImportLegacy { source, id, preview } => return services::print(&claude_core::services::agent_import::import(&context, source, id, *preview)?),
                args::AgentCommands::List => return services::print(&claude_core::services::agents::list(&context)?),
                args::AgentCommands::Import { source } => {
                    let definition = serde_json::from_slice(&std::fs::read(source)?)?;
                    claude_core::services::agents::install(&context, definition)?;
                    return services::print(&serde_json::json!({"imported":true}));
                }
            }
        }
        Some(Commands::Agent { agent_type, prompt }) => {
            claude_core::services::agents::resolve(&services::context(&paths)?, agent_type)?;
            skill_prompt = Some(prompt.clone());
        }
        Some(Commands::MagicDocs { action }) => return magic_docs::run(cli, &paths, action).await,
        Some(Commands::Memory { action }) => return memory::run(cli, &paths, action).await,
        Some(Commands::VoiceConfig { endpoint, model, credential, language }) => {
            let context = services::context(&paths)?;
            let config = claude_core::services::voice::VoiceConfig {
                endpoint: endpoint.clone(), model: model.clone(),
                credential: credential.clone(), language: language.clone(),
            };
            config.validate()?;
            context.save_setting("voice", &config)?;
            return services::print(&config);
        }
        Some(Commands::Voice { file, push_to_talk, continuous, send }) => {
            let context = services::context(&paths)?;
            return voice::run(cli, &paths, &context, file, *push_to_talk, *continuous, *send).await;
        }
        Some(Commands::TeamSync { action }) => {
            return services::team(&services::context(&paths)?, action).await;
        }
        Some(Commands::Services { action }) => {
            return services::services(&services::context(&paths)?, action).await;
        }
        Some(Commands::Config { action }) => return config::run(&paths, action),
        Some(Commands::Mcp { action }) => return mcp(&paths, action).await,
        Some(Commands::Sessions { action }) => {
            let db = paths.open_database()?;
            let value = match action {
                SessionCommands::ImportLegacy {source,preview} => serde_json::to_value(claude_core::services::session_import::import(&services::context(&paths)?,source,&std::env::current_dir()?,*preview)?)?,
                SessionCommands::List => serde_json::to_value(
                    db.with_conn(claude_core::db::conversation_repo::list_conversations)??,
                )?,
                SessionCommands::Show { id } => {
                    let store = SessionStore::new(db);
                    serde_json::json!({"session":store.resume(id)?,"messages":store.history(id)?})
                }
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(());
        }
        Some(Commands::Init { name }) => {
            let cwd = std::env::current_dir()?;
            terminal::init::ensure_directories(&cwd)?;
            let file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(cwd.join("CLAUDE.md"));
            match file {
                Ok(mut file) => writeln!(
                    file,
                    "# {}\n\nAdd project instructions and verification commands here.",
                    name.as_deref().unwrap_or("Project")
                )?,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            eprintln!("Initialized {}", cwd.display());
            return Ok(());
        }
        Some(Commands::Skills { action }) => {
            if let SkillsCommands::ImportLegacy {source,preview} = action {
                return services::print(&claude_core::skills::import::import_legacy(source,&paths.data_dir.join("skills"),*preview)?);
            }
            let manager = SkillsManager::with_path(paths.data_dir.join("skills"));
            let skills = manager.load_skills().await?;
            match action {
                SkillsCommands::ImportLegacy {..} => unreachable!(),
                SkillsCommands::List => {
                    println!("{}", serde_json::to_string_pretty(&skills)?);
                    return Ok(());
                }
                SkillsCommands::Search { query } => {
                    let query = query.to_lowercase();
                    let selected: Vec<_> = skills
                        .iter()
                        .filter(|s| {
                            format!("{} {}", s.name, s.description)
                                .to_lowercase()
                                .contains(&query)
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&selected)?);
                    return Ok(());
                }
                SkillsCommands::Help { skill } | SkillsCommands::Execute { skill, .. } => {
                    let found = skills
                        .iter()
                        .find(|s| &s.id == skill || &s.name == skill)
                        .context("Skill not found")?;
                    let content = manager.get_skill_content(&found.id, "SKILL.md")?;
                    if let SkillsCommands::Execute { args, .. } = action {
                        ensure!(found.enabled, "Skill is disabled");
                        skill_prompt = Some(format!(
                            "Use this skill for the following task:\n\n{content}\n\nTask: {}",
                            args.join(" ")
                        ));
                    } else {
                        println!("{content}");
                        return Ok(());
                    }
                }
            }
        }
        _ => {}
    }
    let explicit_repl = matches!(cli.command, Some(Commands::Repl { .. }));
    let single = cli.print
        || cli.no_interactive
        || matches!(cli.command, Some(Commands::Query { .. }))
        || skill_prompt.is_some()
        || !io::stdin().is_terminal();
    ensure!(
        !explicit_repl || !single,
        "repl requires interactive stdin; use query or --print for scripts"
    );
    ensure!(
        single || cli.output_format == OutputFormat::Text,
        "JSON output requires --print or query"
    );
    let mut prompt = skill_prompt
        .or_else(|| match &cli.command {
            Some(Commands::Repl { prompt }) | Some(Commands::Query { prompt }) => prompt.clone(),
            _ => cli.prompt.clone(),
        })
        .unwrap_or_default();
    if !io::stdin().is_terminal() {
        let mut piped = String::new();
        io::stdin().read_to_string(&mut piped)?;
        if !piped.trim().is_empty() {
            if !prompt.is_empty() {
                prompt.push_str("\n\n");
            }
            prompt.push_str(&piped);
        }
    }
    ensure!(
        !single || !prompt.trim().is_empty(),
        "A prompt or piped stdin is required"
    );
    let resumed = if let Some(id) = &cli.resume {
        Some(SessionStore::new(paths.open_database()?).resume(id)?)
    } else {
        None
    };
    if let Some(resumed) = &resumed {
        let db = paths.open_database()?;
        let (workspace, _) = db.with_conn(|conn| {
            claude_core::native_engine::workspace::resolve_workspace(
                conn,
                &paths.data_dir.join("workspaces"),
                &resumed.id,
                None,
            )
        })??;
        if cli.path.is_some() {
            ensure!(
                std::fs::canonicalize(&workspace)?
                    == std::fs::canonicalize(std::env::current_dir()?)?,
                "--path differs from the resumed session's project"
            );
        }
        std::env::set_current_dir(workspace)?;
    }
    let mut input = if single { None } else { Some(session::terminal_input()) };
    let selection = config::select(
        cli,
        &paths,
        resumed.as_ref().and_then(|s| s.model.as_deref()),
        resumed.as_ref().and_then(|s| s.provider.as_deref()),
    );
    let selection = match selection {
        Ok(selection) => selection,
        Err(error) if !single => {
            eprintln!("Provider setup required: {error:#}");
            interactive_config::setup(&paths, input.as_mut().unwrap()).await?
        },
        Err(error) => return Err(error),
    };
    let mut session = session::Session::open(cli, &paths, selection).await?;
    let result = async {
        if !single { eprintln!("Claude Rust {} — session {}\n/help for commands; Ctrl+C cancels the current task.", env!("CARGO_PKG_VERSION"), session.id); }
        if !prompt.trim().is_empty() { session.turn(cli, &prompt, &mut input).await?; }
        if single { return Ok(()); }
        loop {
            eprint!("\n> "); io::stderr().flush()?;
            let line = tokio::select! {
                line = input.as_mut().expect("interactive reader").recv() => match line { Some(line) => line, None => break },
                _ = tokio::signal::ctrl_c() => break,
            };
            match interactive_config::handle(line.trim(), &paths, &mut session, input.as_mut().unwrap()).await {
                Ok(true) => continue,
                Err(error) => { eprintln!("Error: {error:#}"); continue; },
                Ok(false) => {},
            }
            match line.trim() {
                "" => continue,
                "/exit" | "exit" | "quit" | ".exit" | ":q" => break,
                "/help" | "help" | ".help" | ":h" => eprintln!("/exit  /status  /history  /tools\n/provider [ID|add|edit ID]  /model [ID]  /test\nSwitches preserve history; /test makes an isolated text request. Resume with --resume ID."),
                "/status" | "status" | ".status" => { eprintln!("Session: {}\nProject: {}\nData: {}", session.id, std::env::current_dir()?.display(), paths.data_dir.display()); interactive_config::status(&session); },
                "/history" | "history" | ".history" => println!("{}", serde_json::to_string_pretty(&session.store.history(&session.id)?)?),
                "/tools" | "tools" | ".tools" => for tool in claude_core::tools::get_tool_definitions() { eprintln!("{}: {}", tool.name, tool.description); },
                line => if let Err(error) = session.turn(cli, line, &mut input).await { eprintln!("Error: {error:#}"); },
            }
        }
        Ok(())
    }.await;
    let shutdown = session.close().await;
    result.and(shutdown)
}

async fn mcp(paths: &RuntimePaths, action: &McpCommands) -> Result<()> {
    std::fs::create_dir_all(&paths.data_dir)?;
    let manager = McpServerManager::new(paths.data_dir.join("mcp-servers.json"));
    if let McpCommands::ImportLegacy { source, preview } = action {
        return services::print(&manager.import_legacy(source, *preview).await?);
    }
    manager.initialize().await?;
    match action {
        McpCommands::ImportLegacy { .. } => unreachable!(),
        McpCommands::Hosts => services::print(&claude_core::mcp::control::hosts(&services::context(paths)?)?)?,
        McpCommands::Restart {name,host} => {
            let owner=claude_core::mcp::control::restart(&services::context(paths)?,name,host.as_deref()).await?;
            services::print(&serde_json::json!({"restarted":name,"host":owner}))?;
        }
        McpCommands::List => {
            let mut value = serde_json::to_value(manager.list_servers().await)?;
            config::redact(&mut value);
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        McpCommands::Add {
            name,
            command,
            args,
        } => {
            ensure!(
                manager.get_server(name).await.is_none(),
                "MCP server already exists"
            );
            manager
                .add_server(McpServerConfig {
                    id: name.clone(),
                    name: name.clone(),
                    command: command.clone(),
                    args: args.clone(),
                    env: HashMap::new(),
                    cwd: None,
                    enabled: true,
                })
                .await?;
            eprintln!("Added {name}");
        }
        McpCommands::Remove { name } => {
            if manager.get_server(name).await.is_none() {
                bail!("MCP server not found");
            }
            manager.remove_server(name).await?;
            eprintln!("Removed {name}");
        }
    }
    manager.shutdown_all().await
}
