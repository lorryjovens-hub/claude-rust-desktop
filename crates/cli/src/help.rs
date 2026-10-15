use anyhow::{Context, Result};
use clap::CommandFactory;
pub fn show(topic: Option<&str>, commands: &[String]) -> Result<()> {
    if let Some(topic) = topic {
        let content=match topic {
            "memory"=>"memory import FILE --preview previews an archive; import FILE backs up before writing. memory dream preserves source memories. Configure memory dream-config and run services start auto-dream for scheduled consolidation.",
            "plugins"|"plugin"=>"plugin install SOURCE validates a JSON bundle without running code. plugin search QUERY --registry URL reads an operator catalog. plugin run ID COMMAND --input '{}' --yes executes Node.js/Python with your OS permissions. plugin disable ID also revokes running commands. PluginRun model calls use normal tool permissions.",
            "mcp"=>"mcp import-legacy SETTINGS_JSON --preview checks old MCP definitions; omit --preview to back up and atomically import without launching servers. Conflicting names fail the whole import. Relative working directories must be made absolute first. mcp hosts lists existing live owners. mcp restart NAME --host ID sends a request to that owner and waits for acknowledgement. No Desktop or Bridge is required. Without a running owner restart fails; multiple owners require --host.",
            "voice"=>"voice-config --endpoint URL --model MODEL --credential ENV_NAME selects an audio transcription backend. voice --file recording.wav transcribes a file; --push-to-talk captures microphone audio. Real transcription requires a working audio API and credential.",
            "team-sync"=>"team-sync serve runs the independent team server; TEAM_SERVER_TOKENS supplies the token-to-team map. team-sync auth TEAM --endpoint URL configures a client. team-sync sync exchanges explicitly shared memories; conflicts require resolve. Personal memory and provider credentials are not shared.",
            "magic-docs"=>"magic-docs check FILE registers a project Markdown file beginning with # MAGIC DOC: Title. update ID --context TEXT generates a preview. apply PREVIEW_ID --yes validates permissions and unchanged source before writing. auto ID --enabled opts into automatic updates; start runs the foreground host.",
            "agents"=>"agents list shows built-in and custom definitions. agents import-legacy FILE --id CUSTOM_ID --preview previews old schema/tool mapping; omit --preview to back up and import. Old model/source/base_dir remain provenance; execution uses the selected host model and project. agents import FILE loads {id,description,system_prompt,tools}. agent --agent-type plan --prompt TASK uses the shared engine and tool permissions. The tools list restricts an agent; null inherits host tools.",
            "services"=>"services start team-sync or auto-dream runs in the foreground. services stop NAME signals the actual host across processes. magic-docs start runs its project host. No daemon survives CLI shutdown.",
            "learning"=>"learning configure --enabled opts into metadata-only observations. learning status lists patterns; feedback ID RATING supplies review. learning accept ID installs a reviewed skill in the shared data directory.",
            "performance"=>"performance configure --enabled --provider ID --model ID validates a lightweight model. Use --auto-route for a new CLI conversation; explicit provider/model and resumed sessions are never rerouted.",
            "project"=>"project preview NAME --parent DIR --template rust shows all files without writing. project create uses the same options. Custom template commands require --allow-commands. Existing directories are never overwritten.",
            "remote"|"ssh"=>"remote REQUEST_JSON performs one HTTP request; non-GET/HEAD methods require --yes. ssh CONFIG_JSON OPERATION_JSON uses OpenSSH keys/agent and strict known-host verification; execution/transfers require --yes. Ctrl+C cancels the request.",
            "cron"=>"cron create SCHEDULE PROMPT stores a UTC recurring text-generation task. cron host --provider ID --model ID runs the foreground host; cron stop stops the actual owner across processes. Creating a schedule alone does not run it. Scheduled generations cannot execute tools. Use cron list/get to read durable results.",
            "update"=>"update --url URL checks a publisher-provided JSON manifest using semantic versions. HTTP errors are reported as errors. stress-test -c 4 -i 10 measures real local HTTP response-header latency; it is not a model benchmark.",
            _=>anyhow::bail!("Unknown topic. Available: memory, plugins, mcp, voice, team-sync, magic-docs, agents, services, learning, performance, project, remote, ssh, cron, update"),
        };
        println!("{content}");
        return Ok(());
    }
    let mut command = crate::args::Cli::command();
    for name in commands {
        command = command
            .find_subcommand(name)
            .with_context(|| format!("Unknown command {name}"))?
            .clone();
    }
    command.print_long_help()?;
    println!();
    Ok(())
}
