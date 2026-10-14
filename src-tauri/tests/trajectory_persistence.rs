use claude_desktop_tauri_lib::db::{DbManager, message_repo};
use serde_json::json;

#[test]
fn request_snapshots_survive_reopen_and_follow_message_deletion() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("trajectory.db");
    let snapshot = json!([{"id":"request-1", "number":1, "systemPrompt":"actual injected project instructions", "systemMessages":[{"role":"system","content":"retrieved memory"}], "tools":[{"name":"Read","input_schema":{"type":"object"}}]}]);
    {
        let db = DbManager::new(path.clone())?;
        db.init()?;
        db.with_conn(|conn| -> anyhow::Result<()> {
            conn.execute("INSERT INTO conversations (id, created_at, updated_at) VALUES ('conv', 'now', 'now')", [])?;
            message_repo::insert_message(conn, "assistant", "conv", "assistant", "", None, "now", false, 0)?;
            message_repo::save_request_snapshots(conn, "assistant", &snapshot)?;
            // A snapshot exists even before any answer is produced.
            assert_eq!(message_repo::get_request_snapshots(conn, "assistant")?, snapshot);
            message_repo::update_message_content(conn, "assistant", "answer")?;
            Ok(())
        })??;
    }
    let db = DbManager::new(path)?;
    db.init()?;
    db.with_conn(|conn| -> anyhow::Result<()> {
        assert_eq!(message_repo::get_request_snapshots(conn, "assistant")?, snapshot);
        assert_eq!(message_repo::get_message(conn, "assistant")?.unwrap().content, "answer");
        message_repo::delete_message(conn, "assistant")?;
        assert_eq!(message_repo::get_request_snapshots(conn, "assistant")?, json!([]));
        message_repo::insert_message(conn, "next", "conv", "assistant", "", None, "now", false, 1)?;
        message_repo::save_request_snapshots(conn, "next", &snapshot)?;
        conn.execute("DELETE FROM conversations WHERE id = 'conv'", [])?;
        assert_eq!(message_repo::get_request_snapshots(conn, "next")?, json!([]));
        Ok(())
    })??;
    Ok(())
}

#[test]
fn model_response_metrics_are_independent_sql_columns_and_survive_reopen() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("request-metrics.db");
    let mut snapshot = json!({"id":"request-27", "number":27, "model":"test-model", "providerId":"test-provider", "startedAt":1000, "status":"running", "responseBlocks":[]});
    {
        let db = DbManager::new(path.clone())?;
        db.init()?;
        db.with_conn(|conn| -> anyhow::Result<()> {
            conn.execute("INSERT INTO conversations (id, created_at, updated_at) VALUES ('conv', 'now', 'now')", [])?;
            message_repo::insert_message(conn, "assistant", "conv", "assistant", "", None, "now", false, 0)?;
            message_repo::save_request_snapshots(conn, "assistant", &json!([snapshot]))?;
            let missing: Option<i64> = conn.query_row("SELECT output_tokens FROM model_requests WHERE id='request-27'", [], |row| row.get(0))?;
            assert_eq!(missing, None);
            snapshot.as_object_mut().unwrap().extend(json!({"status":"completed", "endedAt":3070, "firstTokenAt":1976,
                "totalDurationMs":2070,"ttftMs":976,"generationMs":1094,"throughputTokensPerSecond":278.5,
                "inputTokens":100,"outputTokens":305,"reasoningTokens":44,"contentTokens":261,"totalTokens":405,
                "responseBlocks":[{"type":"thinking","text":"Think"},{"type":"text","text":"Answer"},{"type":"tool_call","id":"call","name":"Read","input":{"path":"file"}}],
                "usage":{"prompt_tokens":100,"completion_tokens":305}}).as_object().unwrap().clone());
            message_repo::save_request_snapshots(conn, "assistant", &json!([snapshot]))?;
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM model_requests", [], |row| row.get(0))?;
            assert_eq!(count, 1);
            Ok(())
        })??;
    }
    let db = DbManager::new(path)?;
    db.init()?;
    db.with_conn(|conn| -> anyhow::Result<()> {
        let metrics = conn.query_row("SELECT output_tokens, reasoning_tokens, content_tokens, ttft_ms, generation_ms, status, response_blocks_json FROM model_requests WHERE id='request-27'", [], |row| {
            Ok((row.get::<_,i64>(0)?, row.get::<_,i64>(1)?, row.get::<_,i64>(2)?, row.get::<_,i64>(3)?, row.get::<_,i64>(4)?, row.get::<_,String>(5)?, row.get::<_,String>(6)?))
        })?;
        assert_eq!((metrics.0, metrics.1, metrics.2, metrics.3, metrics.4), (305, 44, 261, 976, 1094));
        assert_eq!(metrics.5, "completed");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&metrics.6)?[2]["name"], "Read");
        assert_eq!(message_repo::get_request_snapshots(conn, "assistant")?, json!([snapshot]));
        message_repo::delete_message(conn, "assistant")?;
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM model_requests", [], |row| row.get::<_,i64>(0))?, 0);
        Ok(())
    })??;
    Ok(())
}
