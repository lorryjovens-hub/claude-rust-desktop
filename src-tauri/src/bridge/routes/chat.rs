use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use axum::response::IntoResponse;
use serde_json;
use std::convert::Infallible;
use tokio::sync::broadcast;
use tokio::time::Duration;

use super::super::{AppState, StreamQuery, set_sse_content_type};
use crate::native_engine::NativeEngine;
use crate::native_engine::engine_core::ChatRequest;
use crate::streaming::{StreamManager, SSE_IDLE_TIMEOUT_SECS, SSE_MAX_DURATION_SECS};
use crate::config::ConfigManager;

pub(super) async fn chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    crate::metrics::HTTP_REQUESTS_TOTAL.inc();
    let _request_timer = crate::metrics::HTTP_REQUEST_DURATION.start_timer();

    let id = req.conversation_id.clone();
    let span = tracing::info_span!("chat_stream", conversation_id = %id);
    let _enter = span.enter();

    {
        let rate_limiter = &state.rate_limiter;
        if !rate_limiter.check("chat_handler").await {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [("X-RateLimit-Remaining", "0"), ("Retry-After", "60")],
                Json(serde_json::json!({"error": "Rate limit exceeded"})),
            ).into_response();
        }
    }

    tracing::info!(module = "Chat", "Handler ENTRY POINT");

    // Reject invalid Code/project requests before persisting messages or invoking a model.
    let db = state.db_manager.clone();
    let project_conversation_id = req.conversation_id.clone();
    let project_check = tokio::task::spawn_blocking(move || db.with_conn(|conn| {
        crate::native_engine::workspace::conversation_project(conn, &project_conversation_id)
    })).await;
    match project_check {
        Ok(Ok(Ok(_))) => {}
        Ok(Ok(Err(error))) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": error.to_string()}))).into_response(),
        _ => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Could not validate conversation project"}))).into_response(),
    }

    let native_engine = state.native_engine.clone();
    let config_manager = state.config_manager.clone();
    let conv_id = req.conversation_id.clone();
    let model = req.model.clone();
    let messages = req.messages;

    tracing::info!(module = "Chat", "[1/7] Parsed request: conv_id={}, model={}, messages={}", conv_id, model, messages.len());

    // Save user messages to SQLite before starting the engine
    tracing::info!(module = "Chat", "[2/7] Starting SQLite save...");
    {
        let db = state.db_manager.clone();
        let conv_id_save = conv_id.clone();
        // The request contains conversation history; only append the current user turn.
        let user_msgs: Vec<_> = messages.iter().rev().find(|m| m.get("role").and_then(|v| v.as_str()) == Some("user")).cloned().into_iter().collect();
        let model_save = model.clone();
        let _ = tokio::task::spawn_blocking(move || {
            db.with_conn(|conn| {
                tracing::info!(module = "BridgeDB", "Saving messages to SQLite for conv_id={}", conv_id_save);
                let now = chrono::Utc::now().to_rfc3339();

                // Step 1: Ensure conversation exists FIRST (before inserting messages)
                let existing = crate::db::conversation_repo::get_conversation(conn, &conv_id_save).ok();
                if existing.flatten().is_none() {
                    tracing::info!(module = "BridgeDB", "Conversation not found, creating new one: id={}, model={}", conv_id_save, model_save);
                    let _ = crate::db::conversation_repo::insert_conversation(
                        conn, &conv_id_save, None, Some(&model_save), None, None, None,
                        false, false, false, &now, &now, 0,
                    );
                    tracing::info!(module = "BridgeDB", "Conversation created successfully");
                } else {
                    tracing::info!(module = "BridgeDB", "Conversation already exists, updating timestamp");
                    // Update the conversation's updated_at timestamp
                    let _ = conn.execute(
                        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
                        rusqlite::params![&now, &conv_id_save],
                    );
                }

                // Step 2: Insert messages
                let mut saved_count = 0;
                for msg in &user_msgs {
                    if let Some(role) = msg.get("role").and_then(|v: &serde_json::Value| v.as_str()) {
                        if role == "user" {
                            let content = msg.get("content")
                                .and_then(|v: &serde_json::Value| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            if !content.is_empty() {
                                let msg_id = uuid::Uuid::new_v4().to_string();
                                let sort_order = crate::db::message_repo::get_messages_by_conversation(conn, &conv_id_save)
                                    .unwrap_or_default()
                                    .len() as i64;
                                tracing::info!(module = "BridgeDB", "Inserting message: id={}, role=user, content_len={}, sort_order={}", msg_id, content.len(), sort_order);
                                let insert_result = crate::db::message_repo::insert_message(
                                    conn, &msg_id, &conv_id_save, "user", &content, None, &now, false, sort_order,
                                );
                                tracing::info!(module = "BridgeDB", "Insert message result: {:?}", insert_result.is_ok());
                                if insert_result.is_ok() {
                                    saved_count += 1;
                                } else {
                                    tracing::error!(module = "BridgeDB", "Insert message FAILED: {:?}", insert_result.err());
                                }
                            }
                        }
                    }
                }
                tracing::info!(module = "BridgeDB", "Saved {} messages to SQLite", saved_count);

                // Step 3: Update conversation message_count
                let total_msgs = crate::db::message_repo::get_messages_by_conversation(conn, &conv_id_save)
                    .unwrap_or_default()
                    .len() as i64;
                let _ = conn.execute(
                    "UPDATE conversations SET message_count = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![total_msgs, &now, &conv_id_save],
                );

                // Step 4: Verify
                let verify_msgs = crate::db::message_repo::get_messages_by_conversation(conn, &conv_id_save);
                tracing::info!(module = "BridgeDB", "Verification: found {} messages in DB for conv_id={}", verify_msgs.as_ref().map(|v| v.len()).unwrap_or(0), conv_id_save);

                Ok::<(), anyhow::Error>(())
            })
        }).await;
    }
    tracing::info!(module = "Chat", "[2/7] SQLite save COMPLETED");

    // Sync providers from ConfigManager to NativeEngine before each request
    tracing::info!(module = "Chat", "[3/7] Syncing providers...");
    let providers_to_sync = {
        let cm_guard: tokio::sync::MutexGuard<'_, Option<ConfigManager>> = config_manager.lock().await;
        if let Some(cm) = cm_guard.as_ref() {
            cm.get_config().providers.iter().map(crate::runtime::provider_from_config).collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    };
    tracing::info!(module = "Chat", "[3/7] Providers synced: count={}", providers_to_sync.len());

    tracing::info!(module = "Chat", "[4/7] Locking native_engine...");
    let mut start_error = "Native engine is not initialized".to_string();
    let rx_opt = {
        let mut engine_guard: tokio::sync::MutexGuard<'_, Option<NativeEngine>> = native_engine.lock().await;
        tracing::info!(module = "Chat", "[4/7] NativeEngine lock acquired");

        if let Some(engine) = engine_guard.as_mut() {
            tracing::info!(module = "Chat", "[5/7] Engine exists, syncing providers and setting permission mode...");

            // Sync latest providers
            engine.sync_providers(providers_to_sync).await;

            let chat_req = crate::native_engine::engine_core::ChatRequest {
                provider_id: req.provider_id.clone(), effort: req.effort.clone(),
                conversation_id: conv_id.clone(),
                messages: messages.clone(),
                model: if model.is_empty() { "claude-sonnet-4-6".to_string() } else { model.clone() },
                system_prompt: req.system_prompt.clone(),
                max_tokens: req.max_tokens,
                workspace_path: req.workspace_path.clone(),
                temperature: req.temperature,
                top_p: req.top_p,
                reasoning_mode: req.reasoning_mode.clone(),
            };

            tracing::info!(module = "Chat", "[6/7] Calling engine.send_message...");

            match engine.send_message_with_history(chat_req, false).await {
                Ok(rx) => {
                    tracing::info!(module = "Chat", "[6/7] send_message SUCCESS, got event receiver");
                    Some(rx)
                }
                Err(e) => {
                    let err_msg = format!("{}", e);
                    tracing::error!(module = "Chat", "NativeEngine send_message error: {}", err_msg);
                    start_error = err_msg;
                    None
                }
            }
        } else {
            tracing::error!(module = "Chat", "NativeEngine not initialized");
            None
        }
    };
    tracing::info!(module = "Chat", "[7/7] Engine guard released, building SSE stream...");

    let (sse_tx, sse_rx) = tokio::sync::mpsc::channel(64);
    let transport = sse_tx.clone();
    let db_for_stream = state.db_manager.clone();
    crate::metrics::ACTIVE_SSE_CONNECTIONS.inc();
    let stream = async_stream::stream! {
        let db_clone = db_for_stream;
        let mut rx = match rx_opt {
            Some(rx) => rx,
            None => {
                tracing::error!(module = "Chat", "rx_opt is None - engine failed to start");
                yield Ok::<Event, Infallible>(Event::default().data(serde_json::json!({"type": "error", "error": start_error}).to_string()));
                return;
            }
        };

        let assistant_id = uuid::Uuid::new_v4().to_string();
        let mut request_snapshots = Vec::<serde_json::Value>::new();
        let db_number = db_clone.clone();
        let conv_number = conv_id.clone();
        let mut request_number = tokio::task::spawn_blocking(move || db_number.with_conn(|conn| {
            conn.query_row("SELECT COALESCE(MAX(request_number), 0) FROM model_requests WHERE conversation_id = ?1", rusqlite::params![conv_number], |row| row.get::<_, i64>(0))
        })).await.ok().and_then(Result::ok).and_then(Result::ok).unwrap_or(0);
        let mut terminal_event = None;
        let mut full_text = String::new();
        let mut collected_tool_calls: Vec<(String, String, String, Option<String>, bool)> = Vec::new(); // (tool_use_id, tool_name, input_json, output, is_error)

        loop {
            let next = tokio::select! {
                biased;
                _ = transport.closed() => {
                    terminal_event = Some(serde_json::json!({"type":"message_stop","stop_reason":"cancelled","full_text":full_text,"error":"Response transport disconnected"}));
                    break;
                }
                event = rx.recv() => event,
            };
            let event = match next {
                Some(e) => e,
                None => {
                    tracing::error!(module = "Chat", "Event channel closed unexpectedly for conv_id={}", conv_id);
                    if full_text.is_empty() {
                        terminal_event = Some(serde_json::json!({
                            "type": "error",
                            "error": "Connection to engine lost. The engine may have crashed."
                        }));
                    } else {
                        terminal_event = Some(serde_json::json!({
                            "type": "message_stop",
                            "stop_reason": "channel_closed",
                            "full_text": full_text.clone(),
                        }));
                    }
                    break;
                }
            };
            let event_data = match event {
                crate::native_engine::tool_loop::EngineEvent::RequestSnapshot(mut snapshot) => {
                    if let Some(existing) = request_snapshots.iter_mut().find(|entry| entry["id"] == snapshot["id"]) {
                        snapshot["number"] = existing["number"].clone();
                        *existing = snapshot.clone();
                    } else {
                        request_number += 1;
                        snapshot["number"] = serde_json::json!(request_number);
                        request_snapshots.push(snapshot.clone());
                    }
                    let db = db_clone.clone();
                    let message_id = assistant_id.clone();
                    let conversation_id = conv_id.clone();
                    let snapshots = serde_json::json!(request_snapshots);
                    let saved = tokio::task::spawn_blocking(move || db.with_conn(|conn| -> anyhow::Result<()> {
                        let tx = conn.unchecked_transaction()?;
                        let now = chrono::Utc::now().to_rfc3339();
                        tx.execute("INSERT OR IGNORE INTO messages (id, conversation_id, role, content, created_at, sort_order) VALUES (?1, ?2, 'assistant', '', ?3, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM messages WHERE conversation_id = ?2))",
                            rusqlite::params![message_id, conversation_id, now])?;
                        crate::db::message_repo::save_request_snapshots(&tx, &message_id, &snapshots)?;
                        tx.commit()?;
                        Ok(())
                    })).await;
                    let persisted = matches!(saved, Ok(Ok(Ok(()))));
                    if !persisted { tracing::error!("Could not persist trajectory request: {:?}", saved); }
                    Some(serde_json::json!({"type": "request_snapshot", "snapshot": snapshot, "persisted": persisted}))
                }
                crate::native_engine::tool_loop::EngineEvent::MessageStart { model } => {
                    Some(serde_json::json!({
                        "type": "message_start",
                        "model": model,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Text(text) => {
                    full_text.push_str(&text);
                    Some(serde_json::json!({
                        "type": "content_block_delta",
                        "delta": {"type": "text_delta", "text": text},
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Thinking(thinking) => {
                    Some(serde_json::json!({
                        "type": "thinking",
                        "thinking": thinking,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::ToolUseStart { tool_use_id, tool_name, tool_input, text_before } => {
                    tracing::info!(module = "Chat", "Tool use started: {} ({})", tool_name, tool_use_id);
                    Some(serde_json::json!({
                        "type": "tool_use_start",
                        "tool_use_id": tool_use_id,
                        "tool_name": tool_name,
                        "tool_input": tool_input,
                        "textBefore": text_before,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::ToolArgDelta { tool_use_id, delta } => {
                    Some(serde_json::json!({
                        "type": "tool_arg_delta",
                        "tool_use_id": tool_use_id,
                        "delta": delta,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::ToolUseDone { tool_use_id, tool_name, tool_input, output, is_error } => {
                    tracing::info!(module = "Chat", "Tool use completed: {} ({}) is_error={}", tool_name, tool_use_id, is_error);
                    let input_json = serde_json::to_string(&tool_input).unwrap_or_default();
                    collected_tool_calls.push((tool_use_id.clone(), tool_name.clone(), input_json, Some(output.clone()), is_error));
                    let db = db_clone.clone();
                    let conversation_id = conv_id.clone();
                    let message_id = assistant_id.clone();
                    let content = full_text.clone();
                    let tool = crate::db::message_repo::ToolCallRow {
                        id: format!("{assistant_id}:{tool_use_id}"), message_id: message_id.clone(), name: tool_name.clone(),
                        input: Some(serde_json::to_string(&tool_input).unwrap_or_default()), output: Some(output.clone()),
                        is_error, sort_order: (collected_tool_calls.len() - 1) as i64,
                    };
                    let saved = tokio::task::spawn_blocking(move || db.with_conn(|conn| {
                        crate::db::message_repo::save_stream_tool_result(conn, &conversation_id, &message_id, &content, &tool)
                    })).await;
                    if !matches!(saved, Ok(Ok(Ok(())))) { tracing::error!("Could not persist tool result: {:?}", saved); }
                    Some(serde_json::json!({
                        "type": "tool_use_done",
                        "tool_use_id": tool_use_id,
                        "tool_name": tool_name,
                        "tool_input": tool_input,
                        "output": output,
                        "content": output,
                        "is_error": is_error,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::MessageDelta { stop_reason } => {
                    Some(serde_json::json!({
                        "type": "message_delta",
                        "delta": {"stop_reason": stop_reason},
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::MessageStop { full_text: _, stop_reason } => {
                    terminal_event = Some(serde_json::json!({
                        "type": "message_stop", "stop_reason": stop_reason, "full_text": full_text.clone(),
                    }));
                    break;
                }
                crate::native_engine::tool_loop::EngineEvent::Error(err) => {
                    tracing::error!(module = "Chat", "Engine error: {}", err);
                    Some(serde_json::json!({
                        "type": "error",
                        "error": err,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::Usage(usage) => {
                    Some(serde_json::json!({
                        "type": "usage",
                        "usage": usage,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::AskUser { request_id, question, options } => {
                    let options_json: Vec<serde_json::Value> = options.iter()
                        .map(|o| serde_json::json!({"label": o, "description": ""}))
                        .collect();
                    Some(serde_json::json!({
                        "type": "ask_user",
                        "request_id": request_id,
                        "tool_use_id": "ask_user_tool",
                        "questions": [{
                            "question": question,
                            "options": options_json
                        }],
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::ToolPermission { request_id, tool_use_id, tool_name, input } => {
                    Some(serde_json::json!({
                        "type": "tool_permission",
                        "request_id": request_id,
                        "tool_use_id": tool_use_id,
                        "tool_name": tool_name,
                        "input": input,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::PipelineToolResult { tool_use_id, tool_name, tool_input, output, is_error, completed_count, total_count } => {
                    Some(serde_json::json!({
                        "type": "pipeline_tool_result",
                        "tool_use_id": tool_use_id,
                        "tool_name": tool_name,
                        "tool_input": tool_input,
                        "output": output,
                        "is_error": is_error,
                        "completed_count": completed_count,
                        "total_count": total_count,
                    }))
                }
                crate::native_engine::tool_loop::EngineEvent::BudgetWarning { message, usage, limit } => {
                    Some(serde_json::json!({
                        "type": "budget_warning",
                        "message": message,
                        "usage": usage,
                        "limit": limit,
                    }))
                }
            };

            if let Some(data) = event_data {
                let is_stop = data.get("type").and_then(|t| t.as_str()) == Some("message_stop")
                    || data.get("type").and_then(|t| t.as_str()) == Some("error");
                if is_stop { terminal_event = Some(data); break; }
                yield Ok::<Event, Infallible>(Event::default().data(data.to_string()));
            }
        }

        // Closing this turn's receiver cancels its executor without addressing a newer turn.
        drop(rx);
        tracing::info!(module = "Chat", "Stream ended for conv_id={}", conv_id);

        crate::metrics::ACTIVE_SSE_CONNECTIONS.dec();

        // A cancelled/crashed engine may never emit its final RequestSnapshot.
        // Persist a terminal status without inventing missing usage or timings.
        let mut pending_snapshot_updates = Vec::new();
        for snapshot in &mut request_snapshots {
            if snapshot["status"] == "running" {
                let error = terminal_event.as_ref().and_then(|event| event.get("error")).cloned();
                snapshot["status"] = serde_json::json!(if error.is_some() { "error" } else { "interrupted" });
                snapshot["endedAt"] = serde_json::json!(chrono::Utc::now().timestamp_millis());
                if let Some(error) = error { snapshot["error"] = error; }
                pending_snapshot_updates.push(snapshot.clone());
            }
        }

        // Save assistant message to SQLite after stream ends
        if !full_text.is_empty() || !request_snapshots.is_empty() || !collected_tool_calls.is_empty() {
            let db = db_clone;
            let conv_id_save = conv_id.clone();
            let assistant_text = full_text.clone();
            let tool_calls_save = collected_tool_calls.clone();
            let snapshots_save = serde_json::json!(request_snapshots);
            let saved = tokio::task::spawn_blocking(move || {
                db.with_conn(|conn| {
                    tracing::info!(module = "BridgeDB", "Saving assistant message for conv_id={}", conv_id_save);
                    let msg_id = assistant_id;
                    let now = chrono::Utc::now().to_rfc3339();
                    let sort_order = crate::db::message_repo::get_messages_by_conversation(conn, &conv_id_save)
                        .unwrap_or_default()
                        .len() as i64;
                    tracing::info!(module = "BridgeDB", "Inserting assistant message: id={}, content_len={}, sort_order={}", msg_id, assistant_text.len(), sort_order);
                    let insert_result = if crate::db::message_repo::get_message(conn, &msg_id)?.is_some() {
                        crate::db::message_repo::update_message_content(conn, &msg_id, &assistant_text)
                    } else {
                        crate::db::message_repo::insert_message(conn, &msg_id, &conv_id_save, "assistant", &assistant_text, None, &now, false, sort_order)
                    };
                    tracing::info!(module = "BridgeDB", "Assistant message insert result: {:?}", insert_result.is_ok());

                    // Save tool calls for this message
                    if insert_result.is_ok() {
                        crate::db::message_repo::save_request_snapshots(conn, &msg_id, &snapshots_save)?;
                        for (i, (tc_id, tc_name, tc_input, tc_output, tc_is_error)) in tool_calls_save.iter().enumerate() {
                            let tc_row_id = format!("{msg_id}:{tc_id}");
                            let _ = crate::db::message_repo::insert_tool_call(
                                conn,
                                &tc_row_id,
                                &msg_id,
                                tc_name,
                                Some(tc_input.as_str()),
                                tc_output.as_deref(),
                                *tc_is_error,
                                i as i64,
                            );
                        }
                        if !tool_calls_save.is_empty() {
                            tracing::info!(module = "BridgeDB", "Saved {} tool_calls for message {}", tool_calls_save.len(), msg_id);
                        }
                    } else { insert_result?; }

                    // Update conversation message_count and updated_at
                    let total_msgs = crate::db::message_repo::get_messages_by_conversation(conn, &conv_id_save)
                        .unwrap_or_default()
                        .len() as i64;
                    let _ = conn.execute(
                        "UPDATE conversations SET message_count = ?1, updated_at = ?2 WHERE id = ?3",
                        rusqlite::params![total_msgs, &now, &conv_id_save],
                    );

                    Ok::<(), anyhow::Error>(())
                })
            }).await;
            let persisted = matches!(saved, Ok(Ok(Ok(()))));
            if !persisted { tracing::error!("Could not persist final assistant trajectory: {:?}", saved); }
            for snapshot in pending_snapshot_updates {
                yield Ok::<Event, Infallible>(Event::default().data(serde_json::json!({"type":"request_snapshot", "snapshot":snapshot, "persisted":persisted}).to_string()));
            }
        }
        if let Some(event) = terminal_event {
            yield Ok::<Event, Infallible>(Event::default().data(event.to_string()));
        }
    };

    // The collector observes transport closure, cancels only its executor, and
    // finishes persisting the partial trajectory before this writer exits.
    tokio::spawn(async move {
        use futures::StreamExt;
        futures::pin_mut!(stream);
        while let Some(event) = stream.next().await {
            let _ = sse_tx.send(event).await;
        }
    });
    let mut response = Sse::new(tokio_stream::wrappers::ReceiverStream::new(sse_rx)).keep_alive(KeepAlive::default()).into_response();
    set_sse_content_type(&mut response);
    response
}

async fn chat_stream_handler(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let stream_manager = state.stream_manager.clone();
    let mut manager: tokio::sync::MutexGuard<'_, StreamManager> = stream_manager.lock().await;

    let receiver = manager.add_listener(&query.conversation_id)
        .ok_or_else(|| StatusCode::NOT_FOUND)?;

    let idle_timeout = Duration::from_secs(SSE_IDLE_TIMEOUT_SECS);
    let max_duration = Duration::from_secs(SSE_MAX_DURATION_SECS);
    let mut last_activity = std::time::Instant::now();

    crate::metrics::ACTIVE_SSE_CONNECTIONS.inc();
    let stream = async_stream::stream! {
        let mut rx = receiver;
        let max_sleep = tokio::time::sleep(max_duration);
        tokio::pin!(max_sleep);

        loop {
            let idle_sleep = tokio::time::sleep(idle_timeout);
            tokio::pin!(idle_sleep);

            tokio::select! {
                _ = &mut max_sleep => {
                    tracing::error!(module = "SSE_chat_stream", "Max duration reached, closing stream");
                    break;
                }
                _ = &mut idle_sleep => {
                    if last_activity.elapsed() >= idle_timeout {
                        tracing::error!(module = "SSE_chat_stream", "Idle timeout, closing stream");
                        break;
                    }
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            last_activity = std::time::Instant::now();
                            let event_name = event.event_type;
                            let data = serde_json::to_string(&event.data).unwrap_or_default();
                            yield Ok::<Event, Infallible>(Event::default()
                                .event(&event_name)
                                .data(data));
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::error!(module = "SSE_chat_stream", "Receiver lagged, dropped {} events", n);
                            last_activity = std::time::Instant::now();
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }

        tracing::info!(module = "SSE_chat_stream", "Stream ended for conv_id={}", query.conversation_id);
        crate::metrics::ACTIVE_SSE_CONNECTIONS.dec();
    };

    let mut response = Sse::new(stream).keep_alive(KeepAlive::default()).into_response();
    set_sse_content_type(&mut response);
    Ok(response)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/chat", post(chat_handler))
        .route("/api/chat/stream", get(chat_stream_handler))
}
