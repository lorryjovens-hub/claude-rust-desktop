use super::{generation::Generator, lifecycle::Lease, ServiceContext};
use anyhow::{Context, Result};
use std::time::Duration;

/// Real recurring model generation. The exclusive project lock prevents duplicate
/// execution even when CLI and Desktop use different application data directories.
pub async fn run(
    context: ServiceContext,
    cwd: String,
    provider: String,
    model: String,
) -> Result<()> {
    let lease = Lease::acquire(&context, "cron")?;
    run_with_lease(context, cwd, provider, model, lease).await
}

pub async fn run_with_lease(
    context: ServiceContext,
    cwd: String,
    provider: String,
    model: String,
    lease: Lease,
) -> Result<()> {
    let cwd = std::path::Path::new(&cwd)
        .canonicalize()?
        .to_string_lossy()
        .into_owned();
    let directory = std::path::Path::new(&cwd).join(".claude");
    crate::tools::resolve_path(&directory.join("cron-host.lock").to_string_lossy(), &cwd)?;
    std::fs::create_dir_all(&directory)?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("cron-host.lock"))?;
    file.try_lock()
        .context("A cron host already owns this project")?;
    let generator = Generator::saved(&context.paths, &provider, &model)?;
    crate::tools::scheduled::recover_interrupted(&cwd)?;
    let mut heartbeat = tokio::time::interval(Duration::from_millis(250));
    loop {
        heartbeat.tick().await;
        if !lease.renew()? {
            break;
        }
        let Some(claim) = crate::tools::scheduled::claim_due(&cwd)? else {
            continue;
        };
        let result = {
            let generation=generator.generate("Complete the scheduled user prompt. This scheduled run generates text only and cannot execute tools or modify files. Do not claim otherwise.",&claim.prompt);
            tokio::pin!(generation);
            loop {
                tokio::select! {
                    result=&mut generation=>break result,
                    _=heartbeat.tick()=>{if !lease.renew()?{return Ok(());}}
                }
            }
        };
        // Drop the request future before moving its claim.
        match result {
            Ok(output) => {
                lease.record(None)?;
                claim.finish(Some(&output), None)?;
            }
            Err(error) => {
                let error = error.to_string();
                lease.record(Some(&error))?;
                claim.finish(None, Some(&error))?;
            }
        }
    }
    Ok(())
}
