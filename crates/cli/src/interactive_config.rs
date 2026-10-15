//! Interactive provider management. Credentials remain environment references.
use crate::{args::Cli, config, session::Session};
use anyhow::{bail, ensure, Context, Result};
use clap::Parser;
use claude_core::{config::ConfigManager, runtime::RuntimePaths};
use serde_json::json;
use std::io::{self, Write};
use tokio::sync::mpsc::UnboundedReceiver;

async fn ask(input: &mut UnboundedReceiver<String>, prompt: &str) -> Result<String> {
    eprint!("{prompt}: ");
    io::stderr().flush()?;
    let line = tokio::select! {
        line = input.recv() => line.context("Input closed")?,
        _ = tokio::signal::ctrl_c() => bail!("Cancelled"),
    };
    ensure!(line.trim() != "/cancel", "Cancelled");
    Ok(line.trim().into())
}

pub async fn wizard(
    paths: &RuntimePaths,
    input: &mut UnboundedReceiver<String>,
    edit: Option<&str>,
) -> Result<()> {
    paths.read_config()?;
    eprintln!("Provider setup; /cancel cancels. Enter environment variable NAMES, never API keys.");
    let mut manager = ConfigManager::new(paths.data_dir.clone());
    let id = match edit {
        Some(id) => id.to_owned(),
        None => ask(input, "Provider ID").await?,
    };
    ensure!(
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "Use letters, digits, - or _ for the ID"
    );
    let previous = manager.get_provider(&id).cloned();
    ensure!(
        edit.is_some() || previous.is_none(),
        "Provider exists; use /provider edit {id}"
    );
    ensure!(edit.is_none() || previous.is_some(), "Provider not found");
    let protocol = ask(input, "Protocol (openai / anthropic / gemini)").await?;
    ensure!(
        ["openai", "anthropic", "claude", "gemini"].contains(&protocol.as_str()),
        "Unsupported protocol"
    );
    let url = ask(input, "Base URL").await?;
    ensure!(
        (url.starts_with("http://") || url.starts_with("https://"))
            && !url.contains('@')
            && !url.contains('?')
            && !url.contains('#'),
        "Use an HTTP(S) URL without credentials, query or fragment"
    );
    let key_env = ask(input, "API key environment variable name").await?;
    ensure!(
        !key_env.is_empty()
            && !key_env.as_bytes()[0].is_ascii_digit()
            && key_env
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "Invalid environment variable name"
    );
    let models = ask(input, "Model IDs (comma separated; first is default)").await?;
    let models: Vec<_> = models
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    ensure!(
        !models.is_empty()
            && models
                .iter()
                .all(|m| !m.contains("::") && !m.contains(char::is_whitespace)),
        "Provide exact model IDs without spaces or ::"
    );
    let mut provider = previous.unwrap_or_else(|| config::provider_template(models[0]));
    provider.id = id.clone();
    provider.name = id.clone();
    provider.provider_type = if protocol == "claude" {
        "anthropic".into()
    } else {
        protocol.clone()
    };
    provider.base_url = url;
    provider.api_key = None;
    provider.enabled = true;
    provider.settings.remove("builtinId");
    provider.settings.insert("apiKeyEnv".into(), json!(key_env));
    provider.models = models
        .iter()
        .map(|id| {
            let mut model = config::provider_template(id).models.remove(0);
            model.id = (*id).into();
            model.name = (*id).into();
            if protocol == "gemini" {
                model.api = Some("google-generative-ai".into());
            }
            model
        })
        .collect();
    if protocol == "gemini" {
        provider
            .settings
            .insert("builtinId".into(), json!("google"));
    }
    manager.update_config(|c| {
        c.providers.retain(|p| p.id != id);
        c.providers.push(provider);
    })?;
    eprintln!(
        "Saved {id} ({} models). Set {key_env} in the launching shell before selecting it.",
        models.len()
    );
    Ok(())
}

pub fn select(paths: &RuntimePaths, provider: &str, model: &str) -> Result<config::Selection> {
    // Deliberately discard startup endpoint/auth overrides on an explicit switch.
    let mut cli = Cli::parse_from(["claude-code"]);
    cli.provider = Some(provider.into());
    cli.model = Some(model.into());
    cli.base_url = None;
    config::select(&cli, paths, None, None)
}

pub fn status(session: &Session) {
    eprintln!(
        "Provider: {}\nModel: {}\nRouting: {}",
        session.provider_id,
        session.model,
        if session.auto_route {
            "automatic"
        } else {
            "selected provider"
        }
    );
}

pub async fn setup(
    paths: &RuntimePaths,
    input: &mut UnboundedReceiver<String>,
) -> Result<config::Selection> {
    loop {
        for p in paths.read_config()?.providers.iter().filter(|p| p.enabled) {
            eprintln!(
                "{}: {}",
                p.id,
                p.models
                    .iter()
                    .filter(|m| m.enabled)
                    .map(|m| m.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let choice = ask(input, "Provider ID, 'add' to configure, or /cancel to exit").await?;
        let result: Result<Option<config::Selection>> = async {
            if choice == "add" {
                wizard(paths, input, None).await?;
                return Ok(None);
            }
            let model = ask(input, "Model ID").await?;
            Ok(Some(select(paths, &choice, &model)?))
        }
        .await;
        match result {
            Ok(Some(selection)) => return Ok(selection),
            Ok(None) => {}
            Err(error) => eprintln!("Error: {error:#}"),
        }
    }
}

pub async fn handle(
    line: &str,
    paths: &RuntimePaths,
    session: &mut Session,
    input: &mut UnboundedReceiver<String>,
) -> Result<bool> {
    let mut words = line.split_whitespace();
    let command = words.next().unwrap_or_default();
    if !["/provider", "/model", "/test"].contains(&command) {
        return Ok(false);
    }
    if command == "/test" {
        ensure!(words.next().is_none(), "Usage: /test");
        status(session);
        eprintln!(
            "Testing a real text request (no tools or conversation history); Ctrl+C cancels."
        );
        let started = std::time::Instant::now();
        let response = tokio::select! {
            result = session.engine.generate_text(&session.provider_id, &session.model, "This is a connection test. Reply only OK.", "Reply OK.") => result?,
            _ = tokio::signal::ctrl_c() => bail!("Test cancelled"),
        };
        eprintln!(
            "Connection test passed in {:.2}s. Response: {}",
            started.elapsed().as_secs_f64(),
            response.chars().take(200).collect::<String>()
        );
        return Ok(true);
    }
    let argument = words.next();
    if command == "/provider" && matches!(argument, Some("add" | "edit")) {
        let edit = if argument == Some("edit") {
            Some(words.next().context("Usage: /provider edit ID")?)
        } else {
            None
        };
        ensure!(words.next().is_none(), "Unexpected argument");
        wizard(paths, input, edit).await?;
        return Ok(true);
    }
    ensure!(
        words.next().is_none(),
        "Use /provider [ID|add|edit ID] or /model [ID]"
    );
    let config = paths.read_config()?;
    let providers: Vec<_> = config.providers.iter().filter(|p| p.enabled).collect();
    let (provider, model) = if command == "/provider" {
        for (i, p) in providers.iter().enumerate() {
            eprintln!("{}. {} — {} ({})", i + 1, p.id, p.name, p.provider_type);
        }
        if argument == Some("list") {
            return Ok(true);
        }
        ensure!(
            !providers.is_empty(),
            "No saved providers; use /provider add"
        );
        let choice = match argument {
            Some(id) => id.to_owned(),
            None => ask(input, "Provider ID or number (blank keeps current)").await?,
        };
        if choice.is_empty() {
            return Ok(true);
        }
        let provider = providers
            .iter()
            .find(|p| p.id == choice)
            .copied()
            .or_else(|| {
                choice
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| providers.get(i).copied())
            })
            .context("Provider not found")?;
        let model = provider
            .models
            .iter()
            .find(|m| m.enabled && m.id == session.model)
            .or_else(|| provider.models.iter().find(|m| m.enabled))
            .context("Provider has no enabled models")?;
        (provider.id.clone(), model.id.clone())
    } else {
        let provider = providers
            .iter()
            .find(|p| p.id == session.provider_id)
            .context("Current provider is temporary; save one with /provider add first")?;
        let models: Vec<_> = provider.models.iter().filter(|m| m.enabled).collect();
        for (i, m) in models.iter().enumerate() {
            eprintln!("{}. {}", i + 1, m.id);
        }
        if argument == Some("list") {
            return Ok(true);
        }
        let choice = match argument {
            Some(id) => id.to_owned(),
            None => ask(input, "Model ID or number (blank keeps current)").await?,
        };
        if choice.is_empty() {
            return Ok(true);
        }
        let model = models
            .iter()
            .find(|m| m.id == choice)
            .copied()
            .or_else(|| {
                choice
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| models.get(i).copied())
            })
            .context("Model not found")?;
        (provider.id.clone(), model.id.clone())
    };
    let selection = select(paths, &provider, &model)?;
    session.switch(paths, selection).await?;
    status(session);
    eprintln!("Conversation history preserved. Use /test to verify connectivity.");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(lines: &[&str]) -> UnboundedReceiver<String> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        for line in lines {
            tx.send((*line).into()).unwrap();
        }
        rx
    }

    #[tokio::test]
    async fn wizard_preserves_other_providers_and_never_saves_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let paths = RuntimePaths::new(dir.path().to_path_buf()).unwrap();
        for (id, protocol) in [("one", "openai"), ("two", "anthropic"), ("three", "gemini")] {
            wizard(
                &paths,
                &mut answers(&[
                    id,
                    protocol,
                    "http://127.0.0.1:8317",
                    "INTERACTIVE_TEST_UNSET_KEY",
                    "model-a,model-b",
                ]),
                None,
            )
            .await
            .unwrap();
        }
        let config = paths.read_config().unwrap();
        assert_eq!(config.providers.len(), 3);
        assert!(config.providers.iter().all(|p| p.api_key.is_none()));
        let gemini = config.providers.iter().find(|p| p.id == "three").unwrap();
        assert_eq!(gemini.settings["builtinId"], "google");
        assert_eq!(
            gemini.models[0].api.as_deref(),
            Some("google-generative-ai")
        );
        assert!(select(&paths, "three", "model-a").is_err());
        let before = std::fs::read(paths.data_dir.join("config.json")).unwrap();
        assert!(wizard(&paths, &mut answers(&["one"]), None).await.is_err());
        assert!(wizard(&paths, &mut answers(&["/cancel"]), Some("one"))
            .await
            .is_err());
        assert_eq!(
            before,
            std::fs::read(paths.data_dir.join("config.json")).unwrap()
        );
        wizard(
            &paths,
            &mut answers(&[
                "openai",
                "http://localhost:9999/v1",
                "INTERACTIVE_TEST_UNSET_KEY",
                "new-model",
            ]),
            Some("one"),
        )
        .await
        .unwrap();
        assert_eq!(paths.read_config().unwrap().providers.len(), 3);
    }

    #[tokio::test]
    async fn switching_and_testing_preserves_history_and_failure_preserves_selection() {
        use axum::{routing::post, Json, Router};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().route("/v1/chat/completions", post(|Json(body): Json<serde_json::Value>| async move {
            assert!(body.get("tools").is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)));
            assert!(!body.to_string().contains("private-history-marker"));
            Json(json!({"id":"test","object":"chat.completion","created":0,"model":"model-a","choices":[{"index":0,"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}}))
        }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let paths = RuntimePaths::new(dir.path().to_path_buf()).unwrap();
        let mut manager = ConfigManager::new(paths.data_dir.clone());
        for id in ["one", "two"] {
            let mut p = config::provider_template("model-a");
            p.id = id.into();
            p.base_url = url.clone();
            p.api_key = Some("mock-only".into());
            manager.add_provider(p).unwrap();
        }
        let cli = Cli::parse_from(["claude-code"]);
        let mut session = Session::open(&cli, &paths, select(&paths, "one", "model-a").unwrap())
            .await
            .unwrap();
        session
            .store
            .append(&session.id, "user", "private-history-marker")
            .unwrap();
        let before = serde_json::to_value(session.store.history(&session.id).unwrap()).unwrap();
        let id = session.id.clone();
        let mut input = answers(&[]);
        handle("/provider two", &paths, &mut session, &mut input)
            .await
            .unwrap();
        assert_eq!(session.provider_id, "two");
        assert!(handle("/model missing", &paths, &mut session, &mut input)
            .await
            .is_err());
        assert_eq!(session.provider_id, "two");
        assert_eq!(session.model, "model-a");
        handle("/test", &paths, &mut session, &mut input)
            .await
            .unwrap();
        assert_eq!(session.id, id);
        assert_eq!(
            before,
            serde_json::to_value(session.store.history(&session.id).unwrap()).unwrap()
        );
        let mut failed = select(&paths, "two", "model-a").unwrap();
        for provider in &mut failed.providers {
            if provider.id == "two" {
                provider.base_url = format!("{url}/missing");
            }
        }
        session.switch(&paths, failed).await.unwrap();
        assert!(handle("/test", &paths, &mut session, &mut input)
            .await
            .is_err());
        assert_eq!(
            before,
            serde_json::to_value(session.store.history(&session.id).unwrap()).unwrap()
        );
        let resumed = session.store.resume(&session.id).unwrap();
        assert_eq!(resumed.provider.as_deref(), Some("two"));
        session.close().await.unwrap();
        server.abort();
    }
}
