use axum::{extract::{Path, State}, http::StatusCode, routing::{get, patch, post}, Json, Router};
use serde_json::{json, Value};
use crate::config::{ModelConfig, ProviderConfig};
use super::super::AppState;

type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;
fn fail(status: StatusCode, message: impl ToString) -> (StatusCode, Json<Value>) { (status, Json(json!({"error": message.to_string()}))) }
fn bad(message: impl ToString) -> (StatusCode, Json<Value>) { fail(StatusCode::BAD_REQUEST, message) }
fn io_error(e: impl ToString) -> (StatusCode, Json<Value>) { fail(StatusCode::INTERNAL_SERVER_ERROR, e) }
fn model_json(m: &ModelConfig) -> Value {
    let mut input = Vec::new();
    if m.supports_text { input.push("text"); }
    if m.supports_vision { input.push("image"); }
    json!({"id":m.id,"name":m.name,"enabled":m.enabled,"contextWindow":m.context_window,"maxTokens":m.max_tokens,"inputTypes":input,"api":m.api})
}
fn provider_json(p: &ProviderConfig) -> Value {
    json!({"id":p.id,"name":p.name,"baseUrl":p.base_url,"apiKey":p.api_key.clone().unwrap_or_default(),"format":p.provider_type,
        "category":p.settings.get("category").unwrap_or(&json!("custom")),"builtinId":p.settings.get("builtinId"),
        "models":p.models.iter().map(model_json).collect::<Vec<_>>(),"enabled":p.enabled,"supportsWebSearch":p.supports_web_search,
        "webSearchStrategy":p.web_search_strategy,"webSearchTestedAt":p.web_search_tested_at,"webSearchTestReason":p.web_search_test_reason})
}
fn token_limit(value: Option<&Value>) -> Result<Option<u32>, String> {
    match value { None | Some(Value::Null) => Ok(None), Some(v) => {
        let n = v.as_u64().filter(|n| *n > 0 && *n <= u32::MAX as u64).ok_or("Token limit must be a positive integer")?;
        Ok(Some(n as u32))
    }}
}
fn parse_models(value: &Value, old: &[ModelConfig]) -> Result<Vec<ModelConfig>, String> {
    let list = value.as_array().ok_or("models must be an array")?;
    let mut seen = std::collections::HashSet::new();
    list.iter().map(|v| {
        let id = v["id"].as_str().unwrap_or("").trim().to_string();
        if id.is_empty() || !seen.insert(id.clone()) { return Err("Model IDs must be nonempty and unique".into()); }
        let previous = old.iter().find(|m| m.id == id);
        if v.get("inputTypes").is_some_and(|i| !i.is_array()) { return Err("inputTypes must be an array".into()); }
        let input = v.get("inputTypes").and_then(Value::as_array);
        if let Some(input) = input { if input.is_empty() || input.iter().any(|t| t != "text" && t != "image") { return Err("Select Text or Image input".into()); } }
        let context_window = if v.get("contextWindow").is_some() { token_limit(v.get("contextWindow"))? } else { previous.and_then(|m| m.context_window) };
        let max_tokens = if v.get("maxTokens").is_some() { token_limit(v.get("maxTokens"))? } else { previous.and_then(|m| m.max_tokens) };
        if let (Some(context), Some(output)) = (context_window, max_tokens) { if output > context { return Err("Max output tokens must not exceed context window".into()); } }
        Ok(ModelConfig { id: id.clone(), name:v["name"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or(&id).to_string(),
            enabled:v["enabled"].as_bool().unwrap_or(true), max_tokens, context_window,
            supports_text:input.map(|a| a.iter().any(|t| t == "text")).unwrap_or_else(|| previous.map_or(true, |m| m.supports_text)),
            supports_vision:input.map(|a| a.iter().any(|t| t == "image")).unwrap_or_else(|| previous.map_or(false, |m| m.supports_vision)),
            api:v["api"].as_str().map(String::from).or_else(|| previous.and_then(|m| m.api.clone())),
            supports_tools:previous.map_or(true, |m| m.supports_tools), supports_streaming:true,
            cost_per_1k_input:previous.and_then(|m| m.cost_per_1k_input), cost_per_1k_output:previous.and_then(|m| m.cost_per_1k_output) })
    }).collect()
}
fn parse_provider(v: Value, old: Option<&ProviderConfig>) -> Result<ProviderConfig, String> {
    let mut p = old.cloned().unwrap_or(ProviderConfig { id:uuid::Uuid::new_v4().to_string(), name:String::new(), provider_type:"openai".into(), api_key:None, base_url:String::new(), models:vec![], enabled:true, is_default:false, settings:Default::default(), supports_web_search:false, web_search_strategy:None, web_search_tested_at:None, web_search_test_reason:None });
    for (key, target) in [("id", &mut p.id), ("name", &mut p.name), ("baseUrl", &mut p.base_url), ("format", &mut p.provider_type)] { if let Some(s) = v.get(key).and_then(Value::as_str) { *target = s.trim().to_string(); } }
    if p.id.is_empty() || !p.id.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) { return Err("Provider ID may contain letters, numbers, dots, underscores and hyphens".into()); }
    if old.is_some_and(|old| old.id != p.id) { return Err("Provider ID cannot be changed after creation".into()); }
    if p.name.is_empty() { return Err("Provider name is required".into()); }
    if !["openai", "anthropic", "harness"].contains(&p.provider_type.as_str()) { return Err("Unsupported protocol".into()); }
    if let Some(key) = v.get("apiKey").and_then(Value::as_str) { p.api_key = Some(key.to_string()); }
    if let Some(enabled) = v.get("enabled").and_then(Value::as_bool) { p.enabled = enabled; }
    for key in ["category", "builtinId"] { if let Some(value) = v.get(key) { p.settings.insert(key.into(), value.clone()); } }
    if p.provider_type == "harness" && p.settings.get("builtinId").and_then(Value::as_str).filter(|s| !s.is_empty()).is_none() { return Err("Built-in provider is required".into()); }
    if p.provider_type == "harness" {
        let catalog: Value = serde_json::from_str(include_str!("../../../../src/data/providerCatalog.json")).map_err(|e| e.to_string())?;
        if !catalog["providers"].as_array().is_some_and(|providers| providers.iter().any(|entry| entry["id"] == p.settings["builtinId"])) { return Err("Unknown built-in provider".into()); }
        p.settings.insert("category".into(), json!("builtin"));
    } else {
        p.settings.insert("category".into(), json!("custom"));
        p.settings.remove("builtinId");
    }
    if !p.base_url.is_empty() { let url = reqwest::Url::parse(&p.base_url).map_err(|_| "Invalid API URL")?; if !["http", "https"].contains(&url.scheme()) { return Err("API URL must use http or https".into()); } }
    else if p.provider_type != "harness" { return Err("API URL is required".into()); }
    if let Some(models) = v.get("models") { p.models = parse_models(models, &p.models)?; }
    Ok(p)
}
async fn providers_list(State(state): State<AppState>) -> ApiResult {
    let guard = state.config_manager.lock().await;
    let cm = guard.as_ref().ok_or_else(|| io_error("Config unavailable"))?;
    Ok(Json(json!({"providers":cm.get_config().providers.iter().map(provider_json).collect::<Vec<_>>()})))
}
async fn providers_create(State(state): State<AppState>, Json(value): Json<Value>) -> ApiResult {
    let provider = parse_provider(value, None).map_err(bad)?;
    let mut guard = state.config_manager.lock().await;
    let cm = guard.as_mut().ok_or_else(|| io_error("Config unavailable"))?;
    if cm.get_provider(&provider.id).is_some() { return Err(fail(StatusCode::CONFLICT, "Provider ID already exists")); }
    cm.add_provider(provider.clone()).map_err(io_error)?;
    Ok(Json(provider_json(&provider)))
}
async fn providers_patch(Path(id): Path<String>, State(state): State<AppState>, Json(value): Json<Value>) -> ApiResult {
    let mut guard = state.config_manager.lock().await;
    let cm = guard.as_mut().ok_or_else(|| io_error("Config unavailable"))?;
    let previous = cm.get_provider(&id).ok_or_else(|| fail(StatusCode::NOT_FOUND, "Provider not found"))?;
    let provider = parse_provider(value, Some(previous)).map_err(bad)?;
    if provider.id != id && cm.get_provider(&provider.id).is_some() { return Err(fail(StatusCode::CONFLICT, "Provider ID already exists")); }
    cm.update_config(|c| { if c.default_provider_id.as_deref() == Some(&id) { c.default_provider_id = Some(provider.id.clone()); } if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) { *p = provider.clone(); } }).map_err(io_error)?;
    Ok(Json(provider_json(&provider)))
}
async fn providers_delete(Path(id): Path<String>, State(state): State<AppState>) -> ApiResult {
    let mut guard = state.config_manager.lock().await;
    let cm = guard.as_mut().ok_or_else(|| io_error("Config unavailable"))?;
    cm.remove_provider(&id).map_err(io_error)?;
    Ok(Json(json!({"ok":true})))
}
async fn providers_models_list(State(state): State<AppState>) -> ApiResult {
    let guard = state.config_manager.lock().await;
    let cm = guard.as_ref().ok_or_else(|| io_error("Config unavailable"))?;
    let models: Vec<Value> = cm.get_config().providers.iter().filter(|p| p.enabled).flat_map(|p| p.models.iter().filter(|m| m.enabled).map(|m| { let mut v = model_json(m); v["providerId"] = json!(p.id); v["providerName"] = json!(p.name); v })).collect();
    Ok(Json(json!({"models":models})))
}
async fn fetch_models(Path(id): Path<String>, State(state): State<AppState>) -> ApiResult {
    let provider = { let guard = state.config_manager.lock().await; guard.as_ref().and_then(|cm| cm.get_provider(&id)).cloned().ok_or_else(|| fail(StatusCode::NOT_FOUND, "Provider not found"))? };
    let base = provider.base_url.trim_end_matches('/');
    let base = base.strip_suffix("/chat/completions").or_else(|| base.strip_suffix("/messages")).unwrap_or(base);
    let endpoint = if base.ends_with("/v1") { format!("{base}/models") } else { format!("{base}/v1/models") };
    let mut request = reqwest::Client::new().get(endpoint).timeout(std::time::Duration::from_secs(30));
    if let Some(key) = provider.api_key.filter(|k| !k.is_empty()) { request = if provider.provider_type == "anthropic" { request.header("x-api-key", key).header("anthropic-version", "2023-06-01") } else { request.bearer_auth(key) }; }
    let response = request.send().await.map_err(io_error)?;
    if !response.status().is_success() { return Err(bad(format!("Model discovery returned HTTP {}", response.status()))); }
    let data: Value = response.json().await.map_err(io_error)?;
    let models: Vec<Value> = data["data"].as_array().ok_or_else(|| bad("Invalid models response"))?.iter().filter_map(|m| m["id"].as_str().map(|id| json!({"id":id,"name":m["name"].as_str().or(m["display_name"].as_str()).unwrap_or(id),"enabled":true,"inputTypes":["text"]}))).collect();
    Ok(Json(json!({"models":models})))
}
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/providers", get(providers_list).post(providers_create))
        .route("/api/providers/{id}", patch(providers_patch).delete(providers_delete))
        .route("/api/providers/models", get(providers_models_list))
        .route("/api/providers/{id}/fetch-models", post(fetch_models))
}

#[cfg(test)]
mod provider_configuration_tests {
    use super::*;
    #[test]
    fn provider_configuration_roundtrip_and_validation() {
        let input = json!({"id":"test-local","name":"Test","baseUrl":"http://127.0.0.1:8317/v1","apiKey":"test-key","format":"openai","models":[{"id":"same-model","name":"Model","contextWindow":1000000,"maxTokens":32000,"inputTypes":["text","image"]}]});
        let provider = parse_provider(input.clone(), None).unwrap();
        assert_eq!(provider.models[0].context_window, Some(1000000));
        assert!(provider.models[0].supports_vision);
        let patch = parse_provider(json!({"models":[{"id":"same-model","name":"Renamed"}]}), Some(&provider)).unwrap();
        assert_eq!(patch.models[0].max_tokens, Some(32000));
        assert!(patch.models[0].supports_vision);
        for invalid in [json!({"inputTypes":[]}),json!({"inputTypes":"text"}),json!({"maxTokens":0}),json!({"contextWindow":100,"maxTokens":101})] {
            let mut value = input.clone();
            value["models"][0].as_object_mut().unwrap().extend(invalid.as_object().unwrap().clone());
            assert!(parse_provider(value,None).is_err());
        }
        assert!(parse_provider(json!({"id":"changed"}),Some(&provider)).is_err());
        let dir = std::env::temp_dir().join(format!("provider-test-{}",uuid::Uuid::new_v4()));
        let mut config = crate::config::ConfigManager::new(dir.clone());
        config.add_provider(provider.clone()).unwrap();
        config.update_config(|c| c.providers[0].name = "Updated".into()).unwrap();
        let reloaded = crate::config::ConfigManager::new(dir.clone());
        assert_eq!(reloaded.get_provider("test-local").unwrap().name,"Updated");
        assert_eq!(reloaded.get_provider("test-local").unwrap().models[0].context_window,Some(1000000));
        std::fs::remove_dir_all(dir).unwrap();
        let path = std::env::temp_dir().join(format!("provider-fail-{}",uuid::Uuid::new_v4()));
        std::fs::write(&path,"occupied").unwrap();
        let mut config = crate::config::ConfigManager::new(path.clone());
        assert!(config.add_provider(provider).is_err());
        assert!(config.get_config().providers.is_empty());
        std::fs::remove_file(path).unwrap();
    }
}
