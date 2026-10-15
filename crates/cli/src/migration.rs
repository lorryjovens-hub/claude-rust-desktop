use anyhow::{ensure, Context, Result};
use clap::{Args, Subcommand};
use claude_core::{
    runtime::RuntimePaths,
    services::{project_templates as projects, remote_access},
};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum CronCommands {
    Create {
        schedule: String,
        prompt: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    List,
    Get {
        id: String,
    },
    Delete {
        id: String,
    },
    /// Run scheduled text generation in the foreground until Ctrl+C or cron stop.
    Host {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        model: String,
    },
    Stop,
}

pub async fn cron(
    paths: &RuntimePaths,
    cwd: &std::path::Path,
    action: &CronCommands,
) -> Result<()> {
    use claude_core::services::{lifecycle, scheduled_prompts};
    let context = crate::services::context(paths)?;
    let cwd = cwd.canonicalize()?.to_string_lossy().into_owned();
    let input = match action {
        CronCommands::Create {
            schedule,
            prompt,
            description,
        } => {
            serde_json::json!({"operation":"create","schedule":schedule,"prompt":prompt,"description":description})
        }
        CronCommands::List => serde_json::json!({"operation":"list"}),
        CronCommands::Get { id } => serde_json::json!({"operation":"get","cron_id":id}),
        CronCommands::Delete { id } => serde_json::json!({"operation":"delete","cron_id":id}),
        CronCommands::Stop => {
            lifecycle::stop(&context, "cron")?;
            lifecycle::wait_stopped(&context, "cron").await?;
            return crate::services::print(&serde_json::json!({"stopped":true}));
        }
        CronCommands::Host { provider, model } => {
            return tokio::select! {
                result=scheduled_prompts::run(context,cwd,provider.clone(),model.clone())=>result,
                _=tokio::signal::ctrl_c()=>Ok(()),
            }
        }
    };
    crate::services::print(&claude_core::tools::scheduled::execute(input, &cwd)?)
}

#[derive(Debug, Subcommand)]
pub enum LearningCommands {
    Status,
    Configure {
        #[arg(long)]
        enabled: bool,
    },
    Feedback {
        id: String,
        rating: f64,
    },
    Accept {
        id: String,
    },
}
#[derive(Debug, Subcommand)]
pub enum PerformanceCommands {
    Status,
    Configure {
        #[arg(long)]
        enabled: bool,
        #[arg(long, default_value = "")]
        provider: String,
        #[arg(long, default_value = "")]
        model: String,
    },
}
#[derive(Debug, Args)]
pub struct ProjectOptions {
    pub name: String,
    #[arg(long, default_value = ".")]
    pub parent: PathBuf,
    #[arg(long, default_value = "basic")]
    pub template: String,
    #[arg(long)]
    pub template_file: Option<PathBuf>,
    #[arg(long, default_value = "{}")]
    pub variables: String,
    #[arg(long)]
    pub git: bool,
    #[arg(long)]
    pub vscode: bool,
}
#[derive(Debug, Subcommand)]
pub enum ProjectCommands {
    Templates,
    Preview(ProjectOptions),
    Create {
        #[command(flatten)]
        options: ProjectOptions,
        #[arg(long)]
        allow_commands: bool,
    },
}

pub async fn learning(paths: &RuntimePaths, action: &LearningCommands) -> Result<()> {
    let ctx = crate::services::context(paths)?;
    match action {
        LearningCommands::Status => {
            let mut value = serde_json::to_value(claude_core::learning::status(&ctx)?)?;
            value["enabled"] =
                serde_json::json!(ctx.setting::<bool>("learning.enabled")?.unwrap_or(false));
            crate::services::print(&value)
        }
        LearningCommands::Configure { enabled } => {
            ctx.save_setting("learning.enabled", enabled)?;
            crate::services::print(&serde_json::json!({"enabled":enabled}))
        }
        LearningCommands::Feedback { id, rating } => {
            claude_core::learning::feedback(&ctx, id, *rating)?;
            crate::services::print(&serde_json::json!({"updated":id}))
        }
        LearningCommands::Accept { id } => {
            crate::services::print(&claude_core::learning::accept(&ctx, id)?)
        }
    }
}
pub async fn performance(paths: &RuntimePaths, action: &PerformanceCommands) -> Result<()> {
    let ctx = crate::services::context(paths)?;
    match action {
        PerformanceCommands::Status => crate::services::print(
            &ctx.setting::<claude_core::performance::RouterConfig>("performance.router")?
                .unwrap_or_default(),
        ),
        PerformanceCommands::Configure {
            enabled,
            provider,
            model,
        } => {
            let config = claude_core::performance::RouterConfig {
                enabled: *enabled,
                provider_id: provider.clone(),
                model_id: model.clone(),
            };
            claude_core::performance::configure_router(&ctx, &config)?;
            crate::services::print(&config)
        }
    }
}
pub async fn project(action: &ProjectCommands) -> Result<()> {
    let (options, create, allow_commands) = match action {
        ProjectCommands::Templates => return crate::services::print(&projects::builtin_templates()),
        ProjectCommands::Preview(options) => (options, false, false),
        ProjectCommands::Create {
            options,
            allow_commands,
        } => (options, true, *allow_commands),
    };
    let template = if let Some(file) = &options.template_file {
        projects::load_template(file)?
    } else {
        projects::builtin_templates()
            .into_iter()
            .find(|t| t.name == options.template)
            .context("Unknown project template")?
    };
    let plan = projects::preview(
        &options.parent,
        &options.name,
        &template,
        &serde_json::from_str(&options.variables)?,
        options.git,
        options.vscode,
    )?;
    if create {
        crate::services::print(
            &serde_json::json!({"created":projects::create(plan,allow_commands).await?}),
        )
    } else {
        crate::services::print(&plan)
    }
}
pub async fn ssh(config: &PathBuf, operation: &PathBuf, yes: bool) -> Result<()> {
    let config: remote_access::SshConfig = serde_json::from_slice(&std::fs::read(config)?)?;
    let operation: remote_access::SshOperation =
        serde_json::from_slice(&std::fs::read(operation)?)?;
    ensure!(
        yes || matches!(operation, remote_access::SshOperation::Check),
        "Inspect operation JSON and pass --yes to execute or transfer files"
    );
    crate::services::print(&remote_access::ssh(&config, operation).await?)
}
pub async fn remote(request: &PathBuf, yes: bool) -> Result<()> {
    let request: remote_access::RemoteRequest = serde_json::from_slice(&std::fs::read(request)?)?;
    ensure!(
        yes || matches!(request.method.to_ascii_uppercase().as_str(), "GET" | "HEAD"),
        "Pass --yes for a mutating HTTP operation"
    );
    crate::services::print(&remote_access::request(request).await?)
}
