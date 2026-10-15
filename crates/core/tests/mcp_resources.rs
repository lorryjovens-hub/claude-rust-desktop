use claude_core::mcp::{McpServerConfig, McpServerManager, McpToolRegistry};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn resource_tools_read_protocol_contents_and_reject_stopped_servers() {
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("resources.cjs");
    std::fs::write(&script, r#"require('readline').createInterface({input:process.stdin}).on('line',line=>{
      const r=JSON.parse(line); const result=r.method==='tools/list'?{tools:[]}:
      r.method==='resources/list'?{resources:[{uri:'fixture://data',name:'data'}]}:
      r.method==='resources/read'?{contents:[{uri:r.params.uri,text:'actual resource text',mimeType:'text/plain'},{uri:r.params.uri,blob:'AQID',mimeType:'application/octet-stream'}]}:
      {protocolVersion:'2024-11-05',capabilities:{resources:{}},serverInfo:{name:'fixture',version:'1'}};
      console.log(JSON.stringify({jsonrpc:'2.0',id:r.id,result}));
    });"#).unwrap();
    let manager = McpServerManager::new(temp.path().join("mcp.json"));
    manager.initialize().await.unwrap();
    manager
        .add_server(McpServerConfig {
            id: "fixture".into(),
            name: "fixture".into(),
            command: "node".into(),
            args: vec![script.to_string_lossy().into_owned()],
            env: Default::default(),
            cwd: None,
            enabled: true,
        })
        .await
        .unwrap();
    manager.start_server("fixture").await.unwrap();
    let manager = Arc::new(manager);
    let registry = McpToolRegistry::new(manager.clone());
    let resources = registry
        .execute_tool("ListMcpResources", json!({"server_id":"fixture"}))
        .await
        .unwrap();
    assert_eq!(resources[0]["uri"], "fixture://data");
    let result = registry
        .execute_tool(
            "ReadMcpResource",
            json!({"server_id":"fixture","uri":"fixture://data"}),
        )
        .await
        .unwrap();
    assert!(result["content"]
        .as_str()
        .unwrap()
        .contains("actual resource text"));
    assert_eq!(result["metadata"]["contents"][1]["blob"], "AQID");
    manager.stop_server("fixture").await.unwrap();
    assert!(registry
        .execute_tool(
            "ReadMcpResource",
            json!({"server_id":"fixture","uri":"fixture://data"})
        )
        .await
        .is_err());
}
