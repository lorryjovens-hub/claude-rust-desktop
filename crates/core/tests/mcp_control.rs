use claude_core::{
    mcp::{control, McpServerConfig, McpServerManager},
    runtime::RuntimePaths,
    services::{EnvironmentCredentials, ServiceContext},
};
use std::{sync::Arc, time::Duration};
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
async fn restart_reaches_existing_process_and_acknowledges_real_restart() {
    if let Ok(root) = std::env::var("MCP_CONTROL_TEST_CHILD") {
        let root = std::path::PathBuf::from(root);
        let manager = McpServerManager::new(root.join("mcp-servers.json"));
        manager.initialize().await.unwrap();
        manager.start_server("fixture").await.unwrap();
        while !root.join("stop").exists() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        manager.shutdown_all().await.unwrap();
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let paths = RuntimePaths::new(root.to_path_buf()).unwrap();
    let context = ServiceContext::open(paths, Arc::new(EnvironmentCredentials)).unwrap();
    let script = root.join("mcp.cjs");
    let boots = root.join("boots.txt");
    std::fs::write(&script,r#"const fs=require('fs');fs.appendFileSync(process.argv[2],process.pid+'\n');require('readline').createInterface({input:process.stdin}).on('line',line=>{let r=JSON.parse(line);console.log(JSON.stringify({jsonrpc:'2.0',id:r.id,result:r.method==='tools/list'?{tools:[]}:r.method==='resources/list'?{resources:[]}:{protocolVersion:'2024-11-05',capabilities:{},serverInfo:{name:'fixture',version:'1'}}}));});"#).unwrap();
    let manager = McpServerManager::new(root.join("mcp-servers.json"));
    manager.initialize().await.unwrap();
    manager
        .add_server(McpServerConfig {
            id: "fixture".into(),
            name: "fixture".into(),
            command: "node".into(),
            args: vec![
                script.to_string_lossy().into(),
                boots.to_string_lossy().into(),
            ],
            env: Default::default(),
            cwd: None,
            enabled: true,
        })
        .await
        .unwrap();
    manager.shutdown_all().await.unwrap();
    assert!(control::restart(&context, "fixture", None).await.is_err());
    let mut child = Child(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "restart_reaches_existing_process_and_acknowledges_real_restart",
                "--nocapture",
            ])
            .env("MCP_CONTROL_TEST_CHILD", root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let host = loop {
        let hosts = control::hosts(&context).unwrap();
        if let Some(host) = hosts
            .into_iter()
            .find(|h| h.servers.iter().any(|s| s == "fixture"))
        {
            break host;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "child did not start"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(host.pid, child.0.id());
    assert_eq!(
        control::restart(&context, "fixture", Some(&host.id))
            .await
            .unwrap(),
        host.id
    );
    let pids = std::fs::read_to_string(boots).unwrap();
    let lines: Vec<_> = pids.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_ne!(lines[0], lines[1]);
    std::fs::write(root.join("stop"), "").unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(control::restart(&context, "fixture", Some(&host.id))
        .await
        .is_err());
}
