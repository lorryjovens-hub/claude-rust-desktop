use anyhow::Result;
use rusqlite::{named_params, Connection};
use serde_json::Value;

/// Numeric metrics have dedicated nullable SQL columns for later aggregation.
pub fn save_request(conn: &Connection, message_id: &str, request: &Value) -> Result<()> {
    let integer = |key: &str| request[key].as_i64();
    let text = |key: &str| request[key].as_str();
    conn.execute(
        "INSERT INTO model_requests (
            id, message_id, conversation_id, request_number, provider_id, model, response_model, api_format, status,
            started_at_ms, first_token_at_ms, ended_at_ms, total_duration_ms, ttft_ms, generation_ms,
            throughput_tokens_per_second, input_tokens, output_tokens, reasoning_tokens, content_tokens,
            total_tokens, cache_read_tokens, cache_write_tokens, max_output_tokens, effort, finish_reason,
            response_id, timing_source, error, response_blocks_json, usage_json
        ) VALUES (
            :id, :message, (SELECT conversation_id FROM messages WHERE id = :message), :number, :provider, :model, :response_model, :format, :status,
            :start, :first, :end, :duration, :ttft, :generation, :throughput, :input, :output, :reasoning, :content,
            :total, :cache_read, :cache_write, :max_output, :effort, :finish, :response_id, :timing_source, :error, :blocks, :usage
        ) ON CONFLICT(id) DO UPDATE SET
            status = excluded.status, response_model = excluded.response_model,
            first_token_at_ms = excluded.first_token_at_ms, ended_at_ms = excluded.ended_at_ms,
            total_duration_ms = excluded.total_duration_ms, ttft_ms = excluded.ttft_ms,
            generation_ms = excluded.generation_ms, throughput_tokens_per_second = excluded.throughput_tokens_per_second,
            input_tokens = excluded.input_tokens, output_tokens = excluded.output_tokens,
            reasoning_tokens = excluded.reasoning_tokens, content_tokens = excluded.content_tokens,
            total_tokens = excluded.total_tokens, cache_read_tokens = excluded.cache_read_tokens,
            cache_write_tokens = excluded.cache_write_tokens, finish_reason = excluded.finish_reason,
            response_id = excluded.response_id, timing_source = excluded.timing_source, error = excluded.error,
            response_blocks_json = excluded.response_blocks_json, usage_json = excluded.usage_json",
        named_params! {
            ":id": text("id"), ":message": message_id, ":number": integer("number"),
            ":provider": text("providerId"), ":model": text("model"), ":response_model": text("responseModel"),
            ":format": text("apiFormat"), ":status": text("status"), ":start": integer("startedAt"),
            ":first": integer("firstTokenAt"), ":end": integer("endedAt"), ":duration": integer("totalDurationMs"),
            ":ttft": integer("ttftMs"), ":generation": integer("generationMs"),
            ":throughput": request["throughputTokensPerSecond"].as_f64(), ":input": integer("inputTokens"),
            ":output": integer("outputTokens"), ":reasoning": integer("reasoningTokens"), ":content": integer("contentTokens"),
            ":total": integer("totalTokens"), ":cache_read": integer("cacheReadTokens"), ":cache_write": integer("cacheWriteTokens"),
            ":max_output": integer("maxOutputTokens"), ":effort": text("effort"), ":finish": text("finishReason"),
            ":response_id": text("responseId"), ":timing_source": text("timingSource"), ":error": text("error"),
            ":blocks": serde_json::to_string(request.get("responseBlocks").unwrap_or(&serde_json::json!([])))?,
            ":usage": request.get("usage").map(serde_json::to_string).transpose()?,
        },
    )?;
    Ok(())
}
