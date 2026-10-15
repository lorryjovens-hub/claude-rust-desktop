use super::super::AppState;
use crate::config::{ConfigManager, DeepSeekSearchConfig};
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

fn failure(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error":message})))
}

fn public_settings(config: &DeepSeekSearchConfig) -> Value {
    json!({"baseUrl":config.base_url, "model":config.model, "hasApiKey":!config.api_key.trim().is_empty()})
}

fn patched_settings(
    previous: &DeepSeekSearchConfig,
    value: &Value,
) -> anyhow::Result<DeepSeekSearchConfig> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("搜索设置必须是 JSON 对象"))?;
    let mut next = previous.clone();
    for (field, target) in [("baseUrl", &mut next.base_url), ("model", &mut next.model)] {
        if let Some(value) = object.get(field) {
            *target = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("{field} 必须是字符串"))?
                .trim()
                .into();
        }
    }
    let clear = match object.get("clearApiKey") {
        None => false,
        Some(value) => value
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("clearApiKey 必须是布尔值"))?,
    };
    if let Some(value) = object.get("apiKey") {
        let key = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("apiKey 必须是字符串"))?
            .trim();
        if clear && !key.is_empty() {
            anyhow::bail!("不能同时设置和清除搜索 API Key");
        }
        // Empty password fields preserve stored credentials. Deletion is explicit.
        if !key.is_empty() {
            next.api_key = key.into();
        }
    }
    if clear {
        next.api_key.clear();
    }
    next.validate()?;
    Ok(next)
}

async fn get_settings(State(state): State<AppState>) -> ApiResult {
    let guard = state.config_manager.lock().await;
    let manager = guard
        .as_ref()
        .ok_or_else(|| failure(StatusCode::SERVICE_UNAVAILABLE, "配置服务暂不可用"))?;
    Ok(Json(public_settings(&manager.get_config().deepseek_search)))
}

fn key_response(config: &DeepSeekSearchConfig) -> (HeaderMap, Json<Value>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    (headers, Json(json!({"apiKey": config.api_key})))
}

// Read the saved key only when the user explicitly chooses View or Copy.
async fn get_key(State(state): State<AppState>) -> Result<(HeaderMap, Json<Value>), (StatusCode, Json<Value>)> {
    let guard = state.config_manager.lock().await;
    let manager = guard.as_ref()
        .ok_or_else(|| failure(StatusCode::SERVICE_UNAVAILABLE, "配置服务暂不可用"))?;
    Ok(key_response(&manager.get_config().deepseek_search))
}

async fn update_settings(State(state): State<AppState>, Json(value): Json<Value>) -> ApiResult {
    let mut guard = state.config_manager.lock().await;
    let manager = guard
        .as_mut()
        .ok_or_else(|| failure(StatusCode::SERVICE_UNAVAILABLE, "配置服务暂不可用"))?;
    let next = patched_settings(&manager.get_config().deepseek_search, &value)
        .map_err(|error| failure(StatusCode::BAD_REQUEST, &error.to_string()))?;
    manager
        .update_config(|config| config.deepseek_search = next.clone())
        .map_err(|_| {
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "保存搜索设置失败，请检查配置目录是否可写",
            )
        })?;
    Ok(Json(public_settings(&next)))
}

async fn run_search_test(
    manager: &tokio::sync::Mutex<Option<ConfigManager>>,
    value: Value,
) -> ApiResult {
    let provider = {
        let guard = manager.lock().await;
        let manager = guard
            .as_ref()
            .ok_or_else(|| failure(StatusCode::SERVICE_UNAVAILABLE, "配置服务暂不可用"))?;
        let draft = patched_settings(&manager.get_config().deepseek_search, &value)
            .map_err(|error| failure(StatusCode::BAD_REQUEST, &error.to_string()))?;
        draft
            .resolved_provider()
            .map_err(|error| failure(StatusCode::BAD_REQUEST, &error.to_string()))?
    };
    // The form is tested without saving it or holding the global config lock
    // while the provider performs the network request.
    let started = std::time::Instant::now();
    let output = crate::tools::search_provider(
        json!({"query":"DeepSeek official API documentation"}),
        &provider,
    )
    .await
    .map_err(|error| failure(StatusCode::BAD_GATEWAY, &error.to_string()))?;
    let result_count = output["results"].as_array().map_or(0, Vec::len);
    Ok(Json(
        json!({"ok":true,"model":output["model"],"resultCount":result_count,
        "elapsedMs":started.elapsed().as_millis(),"results":output["results"]}),
    ))
}

async fn test_settings(State(state): State<AppState>, Json(value): Json<Value>) -> ApiResult {
    run_search_test(&state.config_manager, value).await
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/search-settings",
            get(get_settings).patch(update_settings),
        )
        .route("/api/search-settings/test", post(test_settings))
        .route("/api/search-settings/key", post(get_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_key_response_is_not_cacheable_and_normal_settings_stay_masked() {
        let config = DeepSeekSearchConfig { api_key: "saved-search-key".into(), ..Default::default() };
        let (headers, Json(body)) = key_response(&config);
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(body["apiKey"], "saved-search-key");
        assert!(public_settings(&config).get("apiKey").is_none());
        assert!(!public_settings(&config).to_string().contains("saved-search-key"));
        assert_eq!(key_response(&DeepSeekSearchConfig::default()).1.0["apiKey"], "");
    }

    #[test]
    fn search_settings_preserve_replace_and_clear_key_without_echoing_it() {
        let initial = patched_settings(
            &DeepSeekSearchConfig::default(),
            &json!({"apiKey":"search-only-secret"}),
        )
        .unwrap();
        let changed = patched_settings(
            &initial,
            &json!({"model":"another-search-model","baseUrl":"https://search.example/v1"}),
        )
        .unwrap();
        assert_eq!(changed.api_key, initial.api_key);
        assert_eq!(
            patched_settings(&changed, &json!({"apiKey":""}))
                .unwrap()
                .api_key,
            initial.api_key
        );
        let output = public_settings(&changed);
        assert_eq!(output["hasApiKey"], true);
        assert!(output.get("apiKey").is_none());
        assert!(!output.to_string().contains("search-only-secret"));
        let cleared = patched_settings(&changed, &json!({"clearApiKey":true})).unwrap();
        assert!(cleared.api_key.is_empty());
        for invalid in [
            json!({"apiKey":true}),
            json!({"clearApiKey":"yes"}),
            json!({"model":""}),
            json!({"baseUrl":"https://user:key@example.com"}),
            json!({"baseUrl":"file:///tmp"}),
            json!({"apiKey":"new","clearApiKey":true}),
        ] {
            assert!(patched_settings(&changed, &invalid).is_err());
        }
    }

    #[test]
    fn independent_search_settings_roundtrip_without_changing_chat_providers() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = crate::config::ConfigManager::new(dir.path().into());
        let next = patched_settings(&manager.get_config().deepseek_search, &json!({"apiKey":"search-key", "model":"configured-search", "baseUrl":"https://search.example/anthropic/v1"})).unwrap();
        manager
            .update_config(|config| config.deepseek_search = next)
            .unwrap();
        let reloaded = crate::config::ConfigManager::new(dir.path().into());
        assert_eq!(reloaded.get_config().deepseek_search.api_key, "search-key");
        assert_eq!(
            reloaded.get_config().deepseek_search.model,
            "configured-search"
        );
        assert!(reloaded.get_config().providers.is_empty());
    }

    #[tokio::test]
    async fn search_test_uses_unsaved_form_preserves_stored_key_and_releases_config_lock() {
        use axum::http::HeaderMap;
        use std::sync::Arc;
        use tokio::sync::{mpsc, Mutex};

        let dir = tempfile::tempdir().unwrap();
        let mut config = ConfigManager::new(dir.path().into());
        config
            .update_config(|config| {
                config.deepseek_search.api_key = "saved-search-key".into();
                config.deepseek_search.base_url = "http://127.0.0.1:1/unreachable".into();
                config.deepseek_search.model = "saved-model".into();
            })
            .unwrap();
        let before = std::fs::read(dir.path().join("config.json")).unwrap();
        let manager = Arc::new(Mutex::new(Some(config)));
        let (tx, mut rx) = mpsc::channel::<(HeaderMap, Value)>(2);
        let mock_manager = manager.clone();
        let app = Router::new().route("/custom/messages", post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let tx = tx.clone();
            let manager = mock_manager.clone();
            async move {
                assert!(manager.try_lock().is_ok(), "Search must not hold the configuration lock");
                tx.send((headers, body)).await.unwrap();
                Json(json!({"model":"reported-search-model","content":[{"type":"web_search_tool_result","content":[{"type":"web_search_result","url":"https://api-docs.deepseek.com","title":"API documentation"}]}],"usage":{"input_tokens":10,"output_tokens":3}}))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        for (draft_key, expected_key) in [
            ("", "saved-search-key"),
            ("unsaved-search-key", "unsaved-search-key"),
        ] {
            let Json(result) = run_search_test(
                &manager,
                json!({"baseUrl":format!("http://{address}/custom/messages"),
                "model":"draft-model","apiKey":draft_key}),
            )
            .await
            .unwrap();
            let (headers, body) = rx.recv().await.unwrap();
            assert_eq!(headers["x-api-key"].to_str().unwrap(), expected_key);
            assert_eq!(body["model"], "draft-model");
            assert_eq!(body["tools"][0]["type"], "web_search_20250305");
            assert_eq!(result["resultCount"], 1);
            assert_eq!(result["model"], "reported-search-model");
            assert!(result["elapsedMs"].is_number());
            assert!(result.get("usage").is_none());
            assert!(!result.to_string().contains(expected_key));
        }
        assert_eq!(
            std::fs::read(dir.path().join("config.json")).unwrap(),
            before
        );
        let guard = manager.lock().await;
        let stored = &guard.as_ref().unwrap().get_config().deepseek_search;
        assert_eq!(stored.api_key, "saved-search-key");
        assert_eq!(stored.model, "saved-model");
        server.abort();
    }

    #[tokio::test]
    async fn search_test_missing_key_is_actionable_and_does_not_save() {
        let dir = tempfile::tempdir().unwrap();
        let manager = tokio::sync::Mutex::new(Some(ConfigManager::new(dir.path().into())));
        let (status, Json(error)) = run_search_test(&manager, json!({"clearApiKey":true}))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(error["error"]
            .as_str()
            .unwrap()
            .contains("系统设置 → 联网搜索"));
        assert!(!dir.path().join("config.json").exists());
    }
}
