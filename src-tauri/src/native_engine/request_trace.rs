//! One model response per request. Tool execution time is deliberately excluded.
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Instant};

pub struct RequestTrace {
    snapshot: Value,
    clock: Instant,
    first_content_ms: Option<u64>,
    blocks: Vec<Value>,
    anthropic_indices: BTreeMap<u64, usize>,
    tool_indices: BTreeMap<u64, usize>,
    arguments: BTreeMap<usize, String>,
    usage: Value,
    finish_reason: Option<String>,
    response_id: Option<String>,
    response_finished: bool,
    stream_error: Option<String>,
}

impl RequestTrace {
    pub fn new(mut snapshot: Value) -> Self {
        snapshot["status"] = json!("running");
        snapshot["responseBlocks"] = json!([]);
        Self {
            snapshot,
            clock: Instant::now(),
            first_content_ms: None,
            blocks: vec![],
            anthropic_indices: BTreeMap::new(),
            tool_indices: BTreeMap::new(),
            arguments: BTreeMap::new(),
            usage: json!({}),
            finish_reason: None,
            response_id: None,
            response_finished: false,
            stream_error: None,
        }
    }

    pub fn started(&self) -> Value {
        self.snapshot.clone()
    }

    pub fn mark_completed(&mut self) {
        self.response_finished = true;
    }

    fn observe_error(&mut self, event: &Value) {
        if let Some(error) = event.get("error").filter(|error| !error.is_null()) {
            self.stream_error = Some(
                error["message"]
                    .as_str()
                    .or_else(|| error.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| error.to_string()),
            );
        }
    }

    fn first_content(&mut self) {
        self.first_content_ms
            .get_or_insert(self.clock.elapsed().as_millis() as u64);
    }

    fn merge_usage(&mut self, usage: &Value) {
        if let Some(fields) = usage.as_object() {
            for (key, value) in fields {
                // Anthropic sends input usage at message_start and cumulative output
                // usage at message_delta; replace rather than summing these values.
                self.usage[key] = value.clone();
            }
        }
    }

    fn append_text(&mut self, kind: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        self.first_content();
        if let Some(last) = self.blocks.last_mut().filter(|block| block["type"] == kind) {
            let combined = format!("{}{}", last["text"].as_str().unwrap_or_default(), text);
            last["text"] = json!(combined);
        } else {
            self.blocks.push(json!({"type": kind, "text": text}));
        }
    }

    pub fn observe_openai(&mut self, event: &Value) {
        self.observe_error(event);
        if event["response_content_started"] == true {
            self.first_content();
        }
        if let Some(id) = event["id"].as_str() {
            self.response_id = Some(id.to_string());
        }
        if let Some(model) = event["model"].as_str() {
            self.snapshot["responseModel"] = json!(model);
        }
        if let Some(usage) = event.get("usage") {
            self.merge_usage(usage);
        }
        // Built-in adapters return authoritative final block order and signatures.
        if let Some(content) = event["provider_content"]["content"].as_array() {
            self.blocks = content
                .iter()
                .map(|source| {
                    let mut block = source.clone();
                    match source["type"].as_str().unwrap_or_default() {
                        "thinking" => {
                            block["text"] = source["thinking"].clone();
                        }
                        "toolCall" => {
                            block["type"] = json!("tool_call");
                            block["input"] = source["arguments"].clone();
                            block["arguments"] = json!(source["arguments"].to_string());
                        }
                        _ => {}
                    }
                    block
                })
                .collect();
            self.arguments.clear();
        }
        let Some(choice) = event["choices"]
            .as_array()
            .and_then(|choices| choices.first())
        else {
            return;
        };
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.finish_reason = Some(reason.to_string());
        }
        let delta = &choice["delta"];
        if let Some(text) = delta["reasoning_content"]
            .as_str()
            .or_else(|| delta["reasoning"].as_str())
        {
            self.append_text("thinking", text);
        }
        if let Some(text) = delta["content"].as_str() {
            self.append_text("text", text);
        }
        if let Some(calls) = delta["tool_calls"].as_array() {
            for call in calls {
                self.first_content();
                let index = call["index"].as_u64().unwrap_or(0);
                let position = *self.tool_indices.entry(index).or_insert_with(|| {
                    self.blocks
                        .push(json!({"type": "tool_call", "id": "", "name": "", "input": {}}));
                    self.blocks.len() - 1
                });
                let block = &mut self.blocks[position];
                if let Some(id) = call["id"].as_str() {
                    block["id"] = json!(id);
                }
                if let Some(name) = call["function"]["name"].as_str() {
                    block["name"] = json!(name);
                }
                if let Some(args) = call["function"]["arguments"].as_str() {
                    self.arguments.entry(position).or_default().push_str(args);
                }
            }
        }
    }

    pub fn observe_anthropic(&mut self, event: &Value) {
        self.observe_error(event);
        match event["type"].as_str().unwrap_or_default() {
            "message_stop" => self.mark_completed(),
            "message_start" => {
                if let Some(id) = event["message"]["id"].as_str() {
                    self.response_id = Some(id.to_string());
                }
                if let Some(model) = event["message"]["model"].as_str() {
                    self.snapshot["responseModel"] = json!(model);
                }
                if let Some(usage) = event["message"].get("usage") {
                    self.merge_usage(usage);
                }
            }
            "content_block_start" => {
                let index = event["index"].as_u64().unwrap_or(0);
                let source = &event["content_block"];
                let block = match source["type"].as_str().unwrap_or_default() {
                    "tool_use" => {
                        self.first_content();
                        json!({"type":"tool_call", "id":source["id"], "name":source["name"], "input":source["input"]})
                    }
                    "thinking" => {
                        let text = source["thinking"].as_str().unwrap_or_default();
                        if !text.is_empty() {
                            self.first_content();
                        }
                        let mut block = source.clone();
                        block["text"] = json!(text);
                        block
                    }
                    "text" => {
                        let text = source["text"].as_str().unwrap_or_default();
                        if !text.is_empty() {
                            self.first_content();
                        }
                        source.clone()
                    }
                    _ => source.clone(),
                };
                self.anthropic_indices.insert(index, self.blocks.len());
                self.blocks.push(block);
            }
            "content_block_delta" => {
                let index = event["index"].as_u64().unwrap_or(0);
                let Some(&position) = self.anthropic_indices.get(&index) else {
                    return;
                };
                let delta = &event["delta"];
                if let Some(text) = delta["text"]
                    .as_str()
                    .or_else(|| delta["thinking"].as_str())
                {
                    if !text.is_empty() {
                        self.first_content();
                    }
                    let combined = format!(
                        "{}{}",
                        self.blocks[position]["text"].as_str().unwrap_or_default(),
                        text
                    );
                    self.blocks[position]["text"] = json!(combined);
                }
                if let Some(args) = delta["partial_json"].as_str() {
                    self.arguments.entry(position).or_default().push_str(args);
                }
                if let Some(signature) = delta["signature"].as_str() {
                    let combined = format!(
                        "{}{}",
                        self.blocks[position]["signature"]
                            .as_str()
                            .unwrap_or_default(),
                        signature
                    );
                    self.blocks[position]["signature"] = json!(combined);
                }
            }
            "message_delta" => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.finish_reason = Some(reason.to_string());
                }
                if let Some(usage) = event.get("usage") {
                    self.merge_usage(usage);
                }
            }
            _ => {}
        }
    }

    pub fn finish(mut self, status: &str, error: Option<&str>) -> Value {
        let error = error.map(String::from).or(self.stream_error.take());
        let status = if error.is_some() {
            "error"
        } else if status == "completed" && !self.response_finished && self.finish_reason.is_none() {
            "interrupted"
        } else {
            status
        };
        let duration_ms = self.clock.elapsed().as_millis() as u64;
        let generation_ms = self
            .first_content_ms
            .map(|first| duration_ms.saturating_sub(first));
        for (position, args) in &self.arguments {
            self.blocks[*position]["arguments"] = json!(args);
            self.blocks[*position]["input"] =
                serde_json::from_str::<Value>(args).unwrap_or_else(|_| json!(args));
        }
        let input = self.usage["input_tokens"]
            .as_u64()
            .or_else(|| self.usage["prompt_tokens"].as_u64());
        let output = self.usage["output_tokens"]
            .as_u64()
            .or_else(|| self.usage["completion_tokens"].as_u64());
        let reasoning = self.usage["completion_tokens_details"]["reasoning_tokens"]
            .as_u64()
            .or_else(|| self.usage["output_tokens_details"]["reasoning_tokens"].as_u64())
            .or_else(|| self.usage["reasoning_tokens"].as_u64());
        let content = output
            .zip(reasoning)
            .and_then(|(output, reasoning)| output.checked_sub(reasoning));
        let cache_read = self.usage["cache_read_input_tokens"]
            .as_u64()
            .or_else(|| self.usage["prompt_tokens_details"]["cached_tokens"].as_u64())
            .or_else(|| self.usage["prompt_cache_hit_tokens"].as_u64())
            .or_else(|| self.usage["cached_tokens"].as_u64());
        let cache_write = self.usage["cache_creation_input_tokens"]
            .as_u64()
            .or_else(|| self.usage["prompt_tokens_details"]["cache_write_tokens"].as_u64());
        // Anthropic input_tokens excludes cached input; OpenAI prompt_tokens includes it.
        let total = self.usage["total_tokens"].as_u64().or_else(|| {
            input.zip(output).map(|(input, output)| {
                input
                    + output
                    + if self.usage.get("input_tokens").is_some() {
                        cache_read.unwrap_or(0) + cache_write.unwrap_or(0)
                    } else {
                        0
                    }
            })
        });
        let throughput = output
            .zip(generation_ms)
            .filter(|(_, ms)| *ms > 0)
            .map(|(tokens, ms)| tokens as f64 * 1000.0 / ms as f64);
        let first_at = self.snapshot["startedAt"]
            .as_u64()
            .zip(self.first_content_ms)
            .map(|(start, delay)| start + delay);
        let fields = json!({"status":status, "endedAt":chrono::Utc::now().timestamp_millis(), "firstTokenAt":first_at,
            "totalDurationMs":duration_ms, "ttftMs":self.first_content_ms, "generationMs":generation_ms,
            "throughputTokensPerSecond":throughput, "inputTokens":input, "outputTokens":output,
            "reasoningTokens":reasoning, "contentTokens":content, "totalTokens":total,
            "cacheReadTokens":cache_read, "cacheWriteTokens":cache_write,
            "finishReason":self.finish_reason, "responseId":self.response_id,
            "responseBlocks":self.blocks, "usage":self.usage, "error":error, "timingSource":"engine_monotonic"});
        self.snapshot
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        self.snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn openai_response_keeps_blocks_usage_only_chunks_and_reasoning_split() {
        let mut trace = RequestTrace::new(json!({"id":"request", "startedAt":1000}));
        trace.observe_openai(
            &json!({"id":"response", "choices":[{"delta":{"reasoning_content":"Think"}}]}),
        );
        trace.observe_openai(&json!({"choices":[{"delta":{"content":"Answer"}}]}));
        trace.observe_openai(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call","function":{"name":"Bash","arguments":"{\"command\":"}}]}}]}));
        trace.observe_openai(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"pwd\"}"}}]}, "finish_reason":"tool_calls"}]}));
        trace.observe_openai(&json!({"choices":[],"usage":{"prompt_tokens":100,"completion_tokens":305,"total_tokens":405,"completion_tokens_details":{"reasoning_tokens":44}}}));
        let result = trace.finish("completed", None);
        assert_eq!(
            result["responseBlocks"][2]["input"],
            json!({"command":"pwd"})
        );
        assert_eq!(result["reasoningTokens"], 44);
        assert_eq!(result["contentTokens"], 261);
        assert_eq!(result["totalTokens"], 405);
        assert_eq!(result["responseId"], "response");
        assert_eq!(result["finishReason"], "tool_calls");
        assert!(result["firstTokenAt"].is_number());
    }
    #[test]
    fn anthropic_cumulative_usage_is_not_double_counted_and_unknown_split_is_null() {
        let mut trace = RequestTrace::new(json!({"id":"request", "startedAt":1000}));
        trace.observe_anthropic(&json!({"type":"message_start", "message":{"usage":{"input_tokens":10,"output_tokens":1,"cache_read_input_tokens":20}}}));
        trace.observe_anthropic(&json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}));
        trace.observe_anthropic(&json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"你好"}}));
        trace.observe_anthropic(&json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":30}}));
        let result = trace.finish("completed", None);
        assert_eq!(result["outputTokens"], 30);
        assert_eq!(result["totalTokens"], 60);
        assert!(result["reasoningTokens"].is_null());
        assert!(result["contentTokens"].is_null());
        assert_eq!(result["responseBlocks"][0]["text"], "你好");
    }
    #[test]
    fn no_content_failure_does_not_invent_ttft_or_throughput() {
        let trace = RequestTrace::new(json!({"id":"request", "startedAt":1000}));
        let result = trace.finish("error", Some("Request failed"));
        assert!(result["ttftMs"].is_null());
        assert!(result["throughputTokensPerSecond"].is_null());
        assert_eq!(result["status"], "error");
    }

    #[test]
    fn truncated_response_retains_partial_content_and_is_interrupted() {
        let mut trace = RequestTrace::new(json!({"id":"request", "startedAt":1000}));
        trace.observe_openai(&json!({"choices":[{"delta":{"content":"Partial"}}]}));
        let result = trace.finish("completed", None);
        assert_eq!(result["status"], "interrupted");
        assert_eq!(result["responseBlocks"][0]["text"], "Partial");
        assert!(result["outputTokens"].is_null());
    }

    #[test]
    fn provider_error_event_is_failed_even_when_transport_closes_normally() {
        let mut trace = RequestTrace::new(json!({"id":"request", "startedAt":1000}));
        trace.observe_anthropic(
            &json!({"type":"error", "error":{"type":"overloaded_error", "message":"Overloaded"}}),
        );
        let result = trace.finish("completed", None);
        assert_eq!(result["status"], "error");
        assert_eq!(result["error"], "Overloaded");
    }

    #[test]
    fn native_response_preserves_block_order_signatures_and_tool_input() {
        let mut trace = RequestTrace::new(json!({"id":"request", "startedAt":1000}));
        trace.observe_openai(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{}"}}]}}]}));
        trace.observe_openai(&json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}], "provider_content":{"content":[
            {"type":"thinking","thinking":"Analysis","thinkingSignature":"signature"},
            {"type":"toolCall","id":"call","name":"Read","arguments":{"path":"file"}},
            {"type":"text","text":"Comment"}
        ]}}));
        let result = trace.finish("completed", None);
        assert_eq!(result["responseBlocks"][0]["text"], "Analysis");
        assert_eq!(
            result["responseBlocks"][0]["thinkingSignature"],
            "signature"
        );
        assert_eq!(result["responseBlocks"][1]["input"], json!({"path":"file"}));
        assert_eq!(result["responseBlocks"][2]["text"], "Comment");
    }
}
