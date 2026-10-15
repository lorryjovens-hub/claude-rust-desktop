use claude_core::knowledge::*;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn kb_ingest_conversation(
    flow: State<'_, Arc<FlowEngine>>,
    conv_id: String,
    messages: Vec<serde_json::Value>,
    project_id: Option<String>,
) -> Result<Vec<String>, String> {
    flow.flow
        .ingest_conversation(&conv_id, &messages, project_id.as_deref())
        .await
}

#[tauri::command]
pub async fn kb_prepare_context(
    flow: State<'_, Arc<FlowEngine>>,
    query: String,
    project_id: Option<String>,
) -> Result<String, String> {
    Ok(flow
        .flow
        .prepare_context(&query, project_id.as_deref())
        .await)
}

#[tauri::command]
pub async fn kb_digest(flow: State<'_, Arc<FlowEngine>>) -> Result<String, String> {
    Ok(flow.flow.generate_digest().await)
}

#[tauri::command]
pub async fn kb_patterns(flow: State<'_, Arc<FlowEngine>>) -> Result<Vec<String>, String> {
    Ok(flow.flow.discover_patterns().await)
}

/// ── Project Intelligence Commands ──

#[tauri::command]
pub async fn intel_analyze(
    intel: State<'_, Arc<IntelEngine>>,
    url: String,
) -> Result<ProjectProfile, String> {
    intel.intel.analyze_url(&url).await
}

#[tauri::command]
pub async fn intel_find_fusions(
    intel: State<'_, Arc<IntelEngine>>,
    urls: Vec<String>,
) -> Result<Vec<FusionSuggestion>, String> {
    let mut profiles = Vec::new();
    for url in &urls {
        match intel.intel.analyze_url(url).await {
            Ok(p) => profiles.push(p),
            Err(e) => return Err(format!("分析 {} 失败: {}", url, e)),
        }
    }
    Ok(intel.intel.find_fusions(&profiles))
}

#[tauri::command]
pub async fn intel_generate_skill(
    intel: State<'_, Arc<IntelEngine>>,
    url: String,
) -> Result<SkillBlueprint, String> {
    let profile = intel.intel.analyze_url(&url).await?;
    Ok(intel.intel.generate_skill(&profile).await)
}

#[tauri::command]
pub async fn kb_list(state: State<'_, Arc<KnowledgeBase>>) -> Result<Vec<KnowledgeNode>, String> {
    Ok(state.list_nodes().await)
}

#[tauri::command]
pub async fn kb_get(
    state: State<'_, Arc<KnowledgeBase>>,
    id: String,
) -> Result<Option<KnowledgeNode>, String> {
    Ok(state.get_node(&id).await)
}

#[tauri::command]
pub async fn kb_search(
    state: State<'_, Arc<KnowledgeBase>>,
    query: String,
) -> Result<Vec<KnowledgeNode>, String> {
    Ok(state.search(&query).await)
}

#[tauri::command]
pub async fn kb_add(
    state: State<'_, Arc<KnowledgeBase>>,
    node: KnowledgeNode,
) -> Result<(), String> {
    state.add_node(node).await
}

#[tauri::command]
pub async fn kb_delete(state: State<'_, Arc<KnowledgeBase>>, id: String) -> Result<(), String> {
    state.delete_node(&id).await
}

#[tauri::command]
pub async fn kb_graph(state: State<'_, Arc<KnowledgeBase>>) -> Result<KnowledgeGraph, String> {
    Ok(state.get_graph().await)
}

#[tauri::command]
pub async fn kb_import(
    state: State<'_, Arc<KnowledgeBase>>,
    file_path: String,
    content: String,
) -> Result<String, String> {
    state.import_markdown(&file_path, &content).await
}

/// ── GitHub Hub Commands ──

#[tauri::command]
pub async fn gh_trending(
    hub: State<'_, Arc<GitHubHub>>,
    since: String,
    language: Option<String>,
) -> Result<Vec<claude_core::knowledge::github_hub::GitHubRepo>, String> {
    hub.fetch_trending(&since, language.as_deref()).await
}

#[tauri::command]
pub async fn gh_search(
    hub: State<'_, Arc<GitHubHub>>,
    query: String,
) -> Result<Vec<claude_core::knowledge::github_hub::GitHubRepo>, String> {
    hub.search_repos(&query).await
}

#[tauri::command]
pub async fn gh_user_repos(
    hub: State<'_, Arc<GitHubHub>>,
) -> Result<Vec<claude_core::knowledge::github_hub::GitHubRepo>, String> {
    hub.get_user_repos().await
}

#[tauri::command]
pub async fn gh_set_token(
    hub: State<'_, Arc<GitHubHub>>,
    user_id: String,
    token: String,
) -> Result<(), String> {
    hub.set_token(&user_id, &token).await;
    Ok(())
}

#[tauri::command]
pub async fn gh_watch(hub: State<'_, Arc<GitHubHub>>, full_name: String) -> Result<(), String> {
    hub.watch_repo(&full_name).await
}

#[tauri::command]
pub async fn gh_get_watched(
    hub: State<'_, Arc<GitHubHub>>,
) -> Result<Vec<claude_core::knowledge::github_hub::WatchedRepo>, String> {
    Ok(hub.get_watched().await)
}

#[tauri::command]
pub async fn gh_oauth_url(state_param: String) -> Result<String, String> {
    Ok(claude_core::knowledge::github_hub::GitHubHub::get_oauth_url(&state_param))
}
