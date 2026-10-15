//! A bounded LSP stdio session. Every invocation initializes and shuts down its server.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};

pub(super) fn definition() -> super::ToolDefinition {
    super::ToolDefinition { name:"LSP".into(), description:"Query a real installed language server over LSP stdio. Launching the server requires execution approval. Operations: hover, definition, references, document_symbols, diagnostics. Line and character are zero-based UTF-16 positions. Server errors and missing servers are reported, never fabricated.".into(), input_schema:json!({"type":"object","properties":{
        "server":{"type":"string","description":"Executable, e.g. rust-analyzer, pylsp or typescript-language-server"},"args":{"type":"array","items":{"type":"string"}},"path":{"type":"string"},"language_id":{"type":"string"},"operation":{"type":"string","enum":["hover","definition","references","document_symbols","diagnostics"]},"line":{"type":"integer","minimum":0},"character":{"type":"integer","minimum":0},"timeout_secs":{"type":"integer","minimum":1,"maximum":120}
    },"required":["server","path","language_id","operation"]}) }
}

async fn send(writer: &mut (impl AsyncWrite + Unpin), value: &Value) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

async fn receive(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Value> {
    let mut length = None;
    let mut total = 0;
    loop {
        let mut line = String::new();
        let count = reader.read_line(&mut line).await?;
        if count == 0 {
            bail!("Language server closed stdout");
        }
        total += count;
        if total > 8192 {
            bail!("LSP headers exceed 8 KiB");
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                length = Some(value.trim().parse::<usize>()?);
            }
        }
    }
    let length = length.context("LSP Content-Length is missing")?;
    if length > 8 * 1024 * 1024 {
        bail!("LSP message exceeds 8 MiB");
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn response(
    reader: &mut (impl AsyncBufRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
    id: u64,
) -> Result<Value> {
    loop {
        let message = receive(reader).await?;
        if message.get("method").is_some() {
            if let Some(request_id) = message.get("id") {
                // Reject server-initiated edits and command execution; capabilities are read-only.
                let method = message["method"].as_str().unwrap_or("");
                let result = match method {
                    "workspace/configuration" => Some(json!(message["params"]["items"]
                        .as_array()
                        .map(|items| vec![Value::Null; items.len()])
                        .unwrap_or_default())),
                    "client/registerCapability"
                    | "client/unregisterCapability"
                    | "window/workDoneProgress/create" => Some(Value::Null),
                    _ => None,
                };
                let reply = if let Some(result) = result {
                    json!({"jsonrpc":"2.0","id":request_id,"result":result})
                } else {
                    json!({"jsonrpc":"2.0","id":request_id,"error":{"code":-32601,"message":"Client operation is not supported"}})
                };
                send(writer, &reply).await?;
            }
            continue;
        }
        if message["id"] == id {
            if let Some(error) = message.get("error") {
                bail!("Language server error: {error}");
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

pub(super) async fn execute(mut input: Value, cwd: &str) -> Result<Value> {
    if input.get("operation").is_none() {
        let action = input["action"]
            .as_str()
            .context("operation or legacy action is required")?;
        input["operation"] = json!(if action == "symbols" {
            "document_symbols"
        } else {
            action
        });
    }
    if input.get("server").is_none() || input.get("language_id").is_none() {
        let extension = std::path::Path::new(input["path"].as_str().unwrap_or(""))
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let (server, language, args) = match extension {
            "rs" => ("rust-analyzer", "rust", vec![]),
            "py" => ("pylsp", "python", vec![]),
            "ts" | "tsx" => ("typescript-language-server", "typescript", vec!["--stdio"]),
            "js" | "jsx" => ("typescript-language-server", "javascript", vec!["--stdio"]),
            "c" | "h" => ("clangd", "c", vec![]),
            "cpp" | "cc" | "hpp" => ("clangd", "cpp", vec![]),
            _ => bail!("Specify server and language_id for this file type"),
        };
        if input.get("server").is_none() {
            input["server"] = json!(server);
            if input.get("args").is_none() {
                input["args"] = json!(args);
            }
        }
        if input.get("language_id").is_none() {
            input["language_id"] = json!(language);
        }
    }
    if input.get("line").is_none() {
        input["line"] = json!(0);
    }
    if input.get("character").is_none() {
        input["character"] = json!(0);
    }
    let seconds = input["timeout_secs"].as_u64().unwrap_or(30);
    if !(1..=120).contains(&seconds) {
        bail!("LSP timeout must be 1–120 seconds");
    }
    tokio::time::timeout(std::time::Duration::from_secs(seconds), session(input, cwd))
        .await
        .context("Language server timed out")?
}

async fn session(input: Value, cwd: &str) -> Result<Value> {
    let executable = input["server"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("server is required")?;
    let path = std::path::PathBuf::from(super::resolve_path(
        input["path"].as_str().context("path is required")?,
        cwd,
    )?)
    .canonicalize()?;
    let bytes = tokio::fs::read(&path).await?;
    if bytes.len() > 8 * 1024 * 1024 {
        bail!("LSP document exceeds 8 MiB");
    }
    let text = String::from_utf8(bytes).context("LSP document must be UTF-8")?;
    let uri =
        reqwest::Url::from_file_path(&path).map_err(|_| anyhow::anyhow!("Invalid document URI"))?;
    let root = std::path::Path::new(cwd).canonicalize()?;
    let root_uri = reqwest::Url::from_directory_path(&root)
        .map_err(|_| anyhow::anyhow!("Invalid workspace URI"))?;
    let method = match input["operation"]
        .as_str()
        .context("operation is required")?
    {
        "hover" => "textDocument/hover",
        "definition" => "textDocument/definition",
        "references" => "textDocument/references",
        "document_symbols" => "textDocument/documentSymbol",
        "diagnostics" => "textDocument/diagnostic",
        _ => bail!("Unknown LSP operation"),
    };
    let mut command = tokio::process::Command::new(executable);
    if let Some(args) = input.get("args") {
        for arg in args.as_array().context("args must be an array")? {
            command.arg(arg.as_str().context("args must contain strings")?);
        }
    }
    command
        .current_dir(&root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().context("Cannot start language server")?;
    let mut writer = child.stdin.take().context("Missing server stdin")?;
    let mut reader = BufReader::new(child.stdout.take().context("Missing server stdout")?);
    let result=async {
        send(&mut writer,&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":std::process::id(),"rootUri":root_uri.as_str(),"capabilities":{"general":{"positionEncodings":["utf-16"]},"textDocument":{"diagnostic":{}},"workspace":{"applyEdit":false}},"workspaceFolders":[{"uri":root_uri.as_str(),"name":root.file_name().and_then(|n|n.to_str()).unwrap_or("workspace")}]}})).await?;
        response(&mut reader,&mut writer,1).await?;
        send(&mut writer,&json!({"jsonrpc":"2.0","method":"initialized","params":{}})).await?;
        send(&mut writer,&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"languageId":input["language_id"].as_str().context("language_id is required")?,"version":1,"text":text}}})).await?;
        let mut params=json!({"textDocument":{"uri":uri.as_str()}});
        if !matches!(method,"textDocument/documentSymbol"|"textDocument/diagnostic") { params["position"]=json!({"line":input["line"].as_u64().context("line is required")?,"character":input["character"].as_u64().context("character is required")?}); }
        if method=="textDocument/references" { params["context"]=json!({"includeDeclaration":true}); }
        send(&mut writer,&json!({"jsonrpc":"2.0","id":2,"method":method,"params":params})).await?;
        let value=response(&mut reader,&mut writer,2).await?;
        // Shutdown handshake is bounded independently of an uncooperative server.
        let _=tokio::time::timeout(std::time::Duration::from_secs(1),async {
            send(&mut writer,&json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":null})).await?;
            response(&mut reader,&mut writer,3).await?;
            send(&mut writer,&json!({"jsonrpc":"2.0","method":"exit"})).await
        }).await;
        Ok::<_,anyhow::Error>(json!({"path":path,"operation":input["operation"],"result":value}))
    }.await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn framing_handles_unicode_and_rejects_oversize() {
        let (mut sender, receiver) = tokio::io::duplex(4096);
        let message = json!({"jsonrpc":"2.0","id":2,"result":"中文"});
        send(&mut sender, &message).await.unwrap();
        assert_eq!(
            receive(&mut BufReader::new(receiver)).await.unwrap(),
            message
        );
        let (mut sender, receiver) = tokio::io::duplex(4096);
        sender
            .write_all(b"Content-Length: 99999999\r\n\r\n")
            .await
            .unwrap();
        assert!(receive(&mut BufReader::new(receiver)).await.is_err());
    }
    #[tokio::test]
    async fn real_stdio_fixture_runs_initialize_query_and_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.rs"), "fn main() {}\n").unwrap();
        let script = r#"let buffer=Buffer.alloc(0);process.stdin.on('data',chunk=>{buffer=Buffer.concat([buffer,chunk]);while(true){let end=buffer.indexOf('\r\n\r\n');if(end<0)return;let length=Number(buffer.subarray(0,end).toString().match(/Content-Length: (\d+)/i)[1]);if(buffer.length<end+4+length)return;let m=JSON.parse(buffer.subarray(end+4,end+4+length));buffer=buffer.subarray(end+4+length);if(m.id){let result=m.method==='initialize'?{capabilities:{}}:m.method==='shutdown'?null:{contents:'fixture hover'};let body=JSON.stringify({jsonrpc:'2.0',id:m.id,result});process.stdout.write('Content-Length: '+Buffer.byteLength(body)+'\r\n\r\n'+body);}if(m.method==='exit')process.exit(0);}});"#;
        let result=execute(json!({"server":"node","args":["-e",script],"path":"test.rs","language_id":"rust","operation":"hover","line":0,"character":3}),dir.path().to_str().unwrap()).await.unwrap();
        assert_eq!(result["result"]["contents"], "fixture hover");
    }
}
