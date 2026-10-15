use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct State {
    pub finished: bool,
    blocks: BTreeMap<usize, Value>,
    arguments: BTreeMap<usize, String>,
    items: Vec<Value>,
    usage: Value,
    id: Value,
    model: Value,
    finish: Option<String>,
    tool_index: usize,
    tool_ids: BTreeSet<String>,
}

fn chunk(delta: Value) -> Value {
    json!({"choices":[{"index":0,"delta":delta,"finish_reason":null}]})
}
fn append(value: &mut Value, field: &str, text: &Value) {
    let mut s = value[field].as_str().unwrap_or_default().to_owned();
    s.push_str(text.as_str().unwrap_or_default());
    value[field] = json!(s);
}

impl State {
    pub fn complete(&self, api: &str) -> Option<Value> {
        (api == "bedrock-converse-stream")
            .then(|| self.end(api, json!(self.blocks.values().collect::<Vec<_>>())))
    }
    fn tool(&mut self, id: Value, name: Value, arguments: String) -> Value {
        if let Some(id) = id.as_str() {
            self.tool_ids.insert(id.to_owned());
        }
        let value = chunk(
            json!({"tool_calls":[{"index":self.tool_index,"id":id,"type":"function","function":{"name":name,"arguments":arguments}}]}),
        );
        self.tool_index += 1;
        value
    }
    fn end(&self, api: &str, items: Value) -> Value {
        let mut value = chunk(json!({}));
        value["choices"][0]["finish_reason"] =
            json!(self.finish.as_deref().unwrap_or(if self.tool_index > 0 {
                "tool_calls"
            } else {
                "stop"
            }));
        value["id"] = self.id.clone();
        value["model"] = self.model.clone();
        value["usage"] = self.usage.clone();
        value["provider_content"] = json!({"api":api,"items":items});
        value
    }
    pub fn convert(&mut self, api: &str, mut event: Value) -> Result<Vec<Value>> {
        if !event["error"].is_null() || event["type"] == "error" {
            bail!("Provider stream error: {}", event["error"]);
        }
        if api == "openai-completions" {
            if let Some(choices) = event["choices"].as_array_mut() {
                for choice in choices {
                    if let Some(parts) = choice["delta"]["content"].as_array() {
                        let text = parts
                            .iter()
                            .filter_map(|p| p["text"].as_str())
                            .collect::<String>();
                        let thinking = parts
                            .iter()
                            .filter_map(|p| p["thinking"].as_array())
                            .flatten()
                            .filter_map(|p| p["text"].as_str())
                            .collect::<String>();
                        choice["delta"]["content"] = json!(text);
                        if !thinking.is_empty() {
                            choice["delta"]["reasoning_content"] = json!(thinking);
                        }
                    }
                }
            }
            if event["choices"]
                .as_array()
                .is_some_and(|choices| choices.iter().any(|c| c["finish_reason"].is_string()))
            {
                self.finished = true;
            }
            return Ok(vec![event]);
        }
        if api == "openai-responses" {
            return self.responses(event);
        }
        if api == "bedrock-converse-stream" {
            return self.bedrock(event);
        }
        if api == "google-generative-ai" {
            return self.google(event);
        }
        let mut out = Vec::new();
        let index = event["index"].as_u64().unwrap_or_default() as usize;
        match event["type"].as_str().unwrap_or_default() {
            "message_start" => {
                self.id = event["message"]["id"].clone();
                self.model = event["message"]["model"].clone();
                self.usage = event["message"]["usage"].clone();
                out.push(chunk(json!({"role":"assistant"})));
            }
            "content_block_start" => {
                self.blocks.insert(index, event["content_block"].clone());
                match event["content_block"]["type"].as_str() {
                    Some("text") => {
                        out.push(chunk(json!({"content":event["content_block"]["text"]})))
                    }
                    Some("thinking") => out.push(chunk(
                        json!({"reasoning_content":event["content_block"]["thinking"]}),
                    )),
                    Some("tool_use") => {
                        self.arguments.insert(index, String::new());
                        let mut c = chunk(json!({}));
                        c["response_content_started"] = json!(true);
                        out.push(c);
                    }
                    _ => {}
                }
            }
            "content_block_delta" => {
                let delta = &event["delta"];
                let block = self
                    .blocks
                    .get_mut(&index)
                    .context("Anthropic delta has no content block")?;
                match delta["type"].as_str() {
                    Some("text_delta") => {
                        append(block, "text", &delta["text"]);
                        out.push(chunk(json!({"content":delta["text"]})));
                    }
                    Some("thinking_delta") => {
                        append(block, "thinking", &delta["thinking"]);
                        out.push(chunk(json!({"reasoning_content":delta["thinking"]})));
                    }
                    Some("signature_delta") => append(block, "signature", &delta["signature"]),
                    Some("input_json_delta") => self
                        .arguments
                        .entry(index)
                        .or_default()
                        .push_str(delta["partial_json"].as_str().unwrap_or_default()),
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some(args) = self.arguments.remove(&index) {
                    let block = self
                        .blocks
                        .get_mut(&index)
                        .context("Anthropic tool has no content block")?;
                    if !args.is_empty() {
                        block["input"] = serde_json::from_str(&args)
                            .context("Invalid streamed Anthropic tool arguments")?;
                    }
                    let (id, name, args) = (
                        block["id"].clone(),
                        block["name"].clone(),
                        block["input"].to_string(),
                    );
                    out.push(self.tool(id, name, args));
                }
            }
            "message_delta" => {
                if let Some(u) = event["usage"].as_object() {
                    for (key, value) in u {
                        self.usage[key] = value.clone();
                    }
                }
                self.finish = event["delta"]["stop_reason"].as_str().map(|r| {
                    match r {
                        "tool_use" => "tool_calls",
                        "max_tokens" => "length",
                        "end_turn" | "stop_sequence" => "stop",
                        other => other,
                    }
                    .to_owned()
                });
            }
            "message_stop" => {
                if !self.arguments.is_empty() {
                    bail!("Anthropic stream ended with incomplete tool arguments");
                }
                let input = self.usage["input_tokens"].as_u64().unwrap_or_default();
                let read = self.usage["cache_read_input_tokens"]
                    .as_u64()
                    .unwrap_or_default();
                let write = self.usage["cache_creation_input_tokens"]
                    .as_u64()
                    .unwrap_or_default();
                let output = self.usage["output_tokens"].as_u64().unwrap_or_default();
                self.usage = json!({"prompt_tokens":input+read+write,"completion_tokens":output,"total_tokens":input+read+write+output,"prompt_tokens_details":{"cached_tokens":read},"cache_creation_input_tokens":write});
                self.finished = true;
                out.push(self.end(api, json!(self.blocks.values().collect::<Vec<_>>())));
            }
            _ => {}
        }
        Ok(out)
    }

    fn responses(&mut self, event: Value) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        match event["type"].as_str().unwrap_or_default() {
            "response.created" => {
                self.id = event["response"]["id"].clone();
                self.model = event["response"]["model"].clone();
            }
            "response.output_text.delta" => out.push(chunk(json!({"content":event["delta"]}))),
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                out.push(chunk(json!({"reasoning_content":event["delta"]})))
            }
            "response.output_item.added" if event["item"]["type"] == "function_call" => {
                let mut c = chunk(json!({}));
                c["response_content_started"] = json!(true);
                out.push(c);
            }
            "response.output_item.done" => {
                let item = &event["item"];
                if item["type"] == "function_call" {
                    out.push(
                        self.tool(
                            item["call_id"].clone(),
                            item["name"].clone(),
                            item["arguments"]
                                .as_str()
                                .context("Responses tool call has no arguments")?
                                .to_owned(),
                        ),
                    );
                }
            }
            "response.failed" => bail!("Responses request failed: {}", event["response"]["error"]),
            "response.completed" | "response.incomplete" => {
                let response = &event["response"];
                if response["status"] == "failed" {
                    bail!("Responses request failed: {}", response["error"]);
                }
                if let Some(items) = response["output"].as_array() {
                    for item in items.iter().filter(|i| i["type"] == "function_call") {
                        let id = item["call_id"]
                            .as_str()
                            .context("Responses output call has no call ID")?;
                        if !self.tool_ids.contains(id) {
                            out.push(
                                self.tool(
                                    item["call_id"].clone(),
                                    item["name"].clone(),
                                    item["arguments"]
                                        .as_str()
                                        .context("Responses output call has no arguments")?
                                        .to_string(),
                                ),
                            );
                        }
                    }
                }
                self.id = response["id"].clone();
                self.model = response["model"].clone();
                let u = &response["usage"];
                self.usage = json!({"prompt_tokens":u["input_tokens"].as_u64().unwrap_or_default(),"completion_tokens":u["output_tokens"].as_u64().unwrap_or_default(),"total_tokens":u["total_tokens"].as_u64().unwrap_or_default(),"prompt_tokens_details":u["input_tokens_details"],"completion_tokens_details":u["output_tokens_details"]});
                if event["type"] == "response.incomplete" {
                    self.finish = Some(
                        if response["incomplete_details"]["reason"] == "max_output_tokens" {
                            "length"
                        } else {
                            "content_filter"
                        }
                        .into(),
                    );
                }
                self.finished = true;
                out.push(self.end("openai-responses", response["output"].clone()));
            }
            _ => {}
        }
        Ok(out)
    }

    fn google(&mut self, event: Value) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if !event["promptFeedback"]["blockReason"].is_null() {
            bail!(
                "Gemini blocked prompt: {}",
                event["promptFeedback"]["blockReason"]
            );
        }
        if let Some(candidates) = event["candidates"].as_array() {
            for candidate in candidates.iter().take(1) {
                if let Some(parts) = candidate["content"]["parts"].as_array() {
                    for part in parts {
                        if let Some(text) = part["text"].as_str() {
                            out.push(chunk(if part["thought"] == true {
                                json!({"reasoning_content":text})
                            } else {
                                json!({"content":text})
                            }));
                        }
                        let mut part = part.clone();
                        if part.get("functionCall").is_some() {
                            let call = &mut part["functionCall"];
                            if !call["id"].is_string() {
                                call["id"] = json!(format!("call_{}", uuid::Uuid::new_v4()));
                            }
                            out.push(self.tool(
                                call["id"].clone(),
                                call["name"].clone(),
                                call["args"].to_string(),
                            ));
                        }
                        self.items.push(part);
                    }
                }
                if let Some(reason) = candidate["finishReason"].as_str() {
                    self.finish = Some(
                        match reason {
                            "STOP" if self.tool_index > 0 => "tool_calls",
                            "STOP" => "stop",
                            "MAX_TOKENS" => "length",
                            _ => "content_filter",
                        }
                        .into(),
                    );
                    self.finished = true;
                }
            }
        }
        if let Some(u) = event.get("usageMetadata") {
            self.usage = json!({"prompt_tokens":u["promptTokenCount"].as_u64().unwrap_or_default(),"completion_tokens":u["candidatesTokenCount"].as_u64().unwrap_or_default()+u["thoughtsTokenCount"].as_u64().unwrap_or_default(),"total_tokens":u["totalTokenCount"].as_u64().unwrap_or_default(),"prompt_tokens_details":{"cached_tokens":u["cachedContentTokenCount"].as_u64().unwrap_or_default()},"completion_tokens_details":{"reasoning_tokens":u["thoughtsTokenCount"].as_u64().unwrap_or_default()}});
        }
        if self.finished {
            self.id = event["responseId"].clone();
            self.model = event["modelVersion"].clone();
            out.push(self.end("google-generative-ai", json!(self.items)));
        }
        Ok(out)
    }

    fn bedrock(&mut self, event: Value) -> Result<Vec<Value>> {
        let payload = &event["payload"];
        let index = payload["contentBlockIndex"].as_u64().unwrap_or_default() as usize;
        let mut out = Vec::new();
        match event["type"].as_str().unwrap_or_default() {
            "messageStart" => out.push(chunk(json!({"role":"assistant"}))),
            "contentBlockStart" => {
                if let Some(tool) = payload["start"].get("toolUse") {
                    self.blocks.insert(index, json!({"toolUse":tool}));
                    self.arguments.insert(index, String::new());
                    let mut c = chunk(json!({}));
                    c["response_content_started"] = json!(true);
                    out.push(c);
                }
            }
            "contentBlockDelta" => {
                let delta = &payload["delta"];
                let block = self.blocks.entry(index).or_insert_with(|| json!({}));
                if delta["text"].is_string() {
                    append(block, "text", &delta["text"]);
                    out.push(chunk(json!({"content":delta["text"]})));
                }
                if delta["toolUse"]["input"].is_string() {
                    self.arguments
                        .entry(index)
                        .or_default()
                        .push_str(delta["toolUse"]["input"].as_str().unwrap());
                }
                if let Some(reason) = delta.get("reasoningContent") {
                    if block["reasoningContent"].is_null() {
                        block["reasoningContent"] = json!({"reasoningText":{}});
                    }
                    if reason["text"].is_string() {
                        append(
                            &mut block["reasoningContent"]["reasoningText"],
                            "text",
                            &reason["text"],
                        );
                        out.push(chunk(json!({"reasoning_content":reason["text"]})));
                    }
                    if reason["signature"].is_string() {
                        append(
                            &mut block["reasoningContent"]["reasoningText"],
                            "signature",
                            &reason["signature"],
                        );
                    }
                    if let Some(redacted) = reason.get("redactedContent") {
                        block["reasoningContent"] = json!({"redactedContent":redacted});
                    }
                }
            }
            "contentBlockStop" => {
                if let Some(args) = self.arguments.remove(&index) {
                    let tool = &mut self
                        .blocks
                        .get_mut(&index)
                        .context("Bedrock tool has no start event")?["toolUse"];
                    tool["input"] =
                        serde_json::from_str(if args.is_empty() { "{}" } else { &args })?;
                    let id = tool["toolUseId"].clone();
                    let name = tool["name"].clone();
                    let args = tool["input"].to_string();
                    out.push(self.tool(id, name, args));
                }
            }
            "messageStop" => {
                if !self.arguments.is_empty() {
                    bail!("Bedrock stream ended with incomplete tool arguments");
                }
                self.finish = Some(
                    match payload["stopReason"].as_str().unwrap_or_default() {
                        "end_turn" | "stop_sequence" => "stop",
                        "tool_use" => "tool_calls",
                        "max_tokens" => "length",
                        "guardrail_intervened" | "content_filtered" => "content_filter",
                        other => bail!("Bedrock stopped with {other}"),
                    }
                    .into(),
                );
                self.finished = true;
            }
            "metadata" => {
                let usage = &payload["usage"];
                let read = usage["cacheReadInputTokens"].as_u64().unwrap_or_default();
                let write = usage["cacheWriteInputTokens"].as_u64().unwrap_or_default();
                self.usage = json!({"prompt_tokens":usage["inputTokens"].as_u64().unwrap_or_default(),"completion_tokens":usage["outputTokens"].as_u64().unwrap_or_default(),"total_tokens":usage["totalTokens"].as_u64().unwrap_or_default(),"prompt_tokens_details":{"cached_tokens":read},"cache_creation_input_tokens":write});
            }
            kind if kind.ends_with("Exception") => {
                bail!("Bedrock stream exception {kind}: {}", payload["message"])
            }
            _ => {}
        }
        Ok(out)
    }
}
