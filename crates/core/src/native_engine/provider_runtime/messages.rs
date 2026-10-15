use super::*;
use crate::native_engine::openai_client::{OpenAIContent, OpenAIContentPart};
use serde_json::json;
use std::collections::HashMap;

fn text(content: &OpenAIContent) -> String {
    match content {
        OpenAIContent::Text(s) => s.clone(),
        OpenAIContent::Multi(parts) => parts
            .iter()
            .filter_map(|p| match p {
                OpenAIContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn parts(content: &OpenAIContent, api: &str, assistant: bool) -> Result<Vec<Value>> {
    let source = match content {
        OpenAIContent::Text(s) => vec![OpenAIContentPart::Text { text: s.clone() }],
        OpenAIContent::Multi(p) => p.clone(),
    };
    source.into_iter().map(|p| Ok(match p {
        OpenAIContentPart::Text{text} => match api {
            "openai-responses" => json!({"type":if assistant {"output_text"} else {"input_text"},"text":text}),
            "google-generative-ai" => json!({"text":text}),
            _ => json!({"type":"text","text":text}),
        },
        OpenAIContentPart::Image{image_url} => {
            if api == "openai-responses" { json!({"type":"input_image","image_url":image_url.url}) }
            else {
                let (header,data) = image_url.url.split_once(",").context("Image must contain base64 data")?;
                let mime = header.strip_prefix("data:").and_then(|s| s.strip_suffix(";base64")).context("Image must contain a base64 data URL")?;
                if api == "google-generative-ai" { json!({"inlineData":{"mimeType":mime,"data":data}}) }
                else { json!({"type":"image","source":{"type":"base64","media_type":mime,"data":data}}) }
            }
        }
    })).collect()
}

pub(super) fn request(
    api: &str,
    provider: &ResolvedProvider,
    metadata: &Value,
    messages: Vec<OpenAIMessage>,
    system: Option<&str>,
    tools: Vec<ToolDefinition>,
    max_tokens: u32,
) -> Result<Value> {
    let mut systems = system.map(str::to_owned).into_iter().collect::<Vec<_>>();
    systems.extend(
        messages
            .iter()
            .filter(|m| m.role == "system")
            .map(|m| text(&m.content)),
    );
    let system = systems.join("\n\n");
    let messages = messages
        .into_iter()
        .filter(|m| m.role != "system")
        .collect::<Vec<_>>();
    let effort = provider.model.effort.as_deref();
    if api == "bedrock-converse-stream" {
        return bedrock(provider, messages, &system, tools, max_tokens);
    }
    if api == "openai-completions" {
        let mut input = Vec::new();
        if !system.is_empty() {
            input.push(json!({"role":"system","content":system}));
        }
        for message in messages {
            let mut item = serde_json::to_value(message)?;
            item.as_object_mut().unwrap().remove("provider_content");
            input.push(item);
        }
        let mut body = json!({"model":provider.model.id,"messages":input,"stream":true,"stream_options":{"include_usage":true}});
        let field = metadata["compat"]["maxTokensField"]
            .as_str()
            .unwrap_or("max_tokens");
        body[field] = json!(max_tokens);
        if !tools.is_empty() {
            body["tools"] = tools.iter().map(|t| json!({"type":"function","function":{"name":t.name,"description":t.description,"parameters":t.input_schema}})).collect();
        }
        if let Some(effort) = effort {
            let mapped = metadata["thinkingLevelMap"][effort]
                .as_str()
                .unwrap_or(effort);
            match metadata["compat"]["thinkingFormat"].as_str() {
                Some("deepseek") => {
                    body["thinking"] =
                        json!({"type":if effort == "off" {"disabled"} else {"enabled"}});
                    if effort != "off" {
                        body["reasoning_effort"] = json!(mapped);
                    }
                }
                Some("qwen") => {
                    body["enable_thinking"] = json!(effort != "off");
                }
                Some("zai") => {
                    body["thinking"] =
                        json!({"type":if effort == "off" {"disabled"} else {"enabled"}});
                }
                _ => {
                    body["reasoning_effort"] = json!(if effort == "off" { "none" } else { mapped });
                }
            }
        }
        return Ok(body);
    }
    let mut input = Vec::new();
    let mut names = HashMap::new();
    for message in messages {
        if let Some(calls) = &message.tool_calls {
            for call in calls {
                names.insert(call.id.clone(), call.function.name.clone());
            }
        }
        // Replay exact provider blocks, including encrypted reasoning/signatures, within a tool loop.
        if message.role == "assistant" {
            if let Some(native) = &message.provider_content {
                if native["api"] == api {
                    if let Some(items) = native["items"].as_array() {
                        if api == "openai-responses" {
                            input.extend(items.clone());
                        } else if api == "google-generative-ai" {
                            input.push(json!({"role":"model","parts":items}));
                        } else {
                            input.push(json!({"role":"assistant","content":items}));
                        }
                        continue;
                    }
                }
                // Existing pi-ai sessions must not silently drop signed thinking.
                if native.get("content").is_some() {
                    bail!("This conversation contains legacy provider-native blocks; start a new session for the native Rust transport");
                }
            }
        }
        if message.role == "tool" {
            let id = message
                .tool_call_id
                .as_deref()
                .context("Tool result is missing its call ID")?;
            input.push(match api {
                "openai-responses" => json!({"type":"function_call_output","call_id":id,"output":text(&message.content)}),
                "google-generative-ai" => json!({"role":"user","parts":[{"functionResponse":{"id":id,"name":names.get(id).context("Gemini tool result has no matching function call")?,"response":{"result":text(&message.content)}}}]}),
                _ => json!({"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":text(&message.content)}]}),
            });
            continue;
        }
        let mut content = parts(&message.content, api, message.role == "assistant")?;
        if let Some(calls) = &message.tool_calls {
            for call in calls {
                if api == "openai-responses" {
                    continue;
                }
                let args: Value = serde_json::from_str(&call.function.arguments)
                    .context("Tool call arguments are not valid JSON")?;
                content.push(if api == "google-generative-ai" {
                    json!({"functionCall":{"id":call.id,"name":call.function.name,"args":args}})
                } else {
                    json!({"type":"tool_use","id":call.id,"name":call.function.name,"input":args})
                });
            }
        }
        if api == "google-generative-ai" {
            input.push(json!({"role":if message.role == "assistant" {"model"} else {"user"},"parts":content}));
        } else if !content.is_empty() {
            input.push(json!({"role":message.role,"content":content}));
        }
        if api == "openai-responses" {
            if let Some(calls) = message.tool_calls {
                for call in calls {
                    input.push(json!({"type":"function_call","call_id":call.id,"name":call.function.name,"arguments":call.function.arguments}));
                }
            }
        }
    }
    let mut body = match api {
        "openai-responses" => {
            let mut body = json!({"model":provider.model.id,"input":input,"instructions":system,"stream":true,"store":false,"max_output_tokens":max_tokens,"include":["reasoning.encrypted_content"]});
            if !tools.is_empty() {
                body["tools"] = tools.iter().map(|t| json!({"type":"function","name":t.name,"description":t.description,"parameters":t.input_schema,"strict":false})).collect();
            }
            if let Some(effort) = effort {
                body["reasoning"] =
                    json!({"effort":if effort == "off" {"none"} else {effort},"summary":"auto"});
            }
            body
        }
        "google-generative-ai" => {
            let mut body =
                json!({"contents":input,"generationConfig":{"maxOutputTokens":max_tokens}});
            if !system.is_empty() {
                body["systemInstruction"] = json!({"parts":[{"text":system}]});
            }
            if !tools.is_empty() {
                body["tools"] = json!([{"functionDeclarations":tools.iter().map(|t| json!({"name":t.name,"description":t.description,"parametersJsonSchema":t.input_schema})).collect::<Vec<_>>()}]);
            }
            if let Some(effort) = effort {
                body["generationConfig"]["thinkingConfig"] = if provider
                    .model
                    .id
                    .contains("gemini-3")
                {
                    json!({"thinkingLevel":if effort == "off" {"minimal"} else {effort},"includeThoughts":true})
                } else {
                    json!({"thinkingBudget":match effort {"off"=>0,"low"=>1024,"medium"=>8192,_=>16384},"includeThoughts":true})
                };
            }
            body
        }
        _ => {
            let mut body = json!({"model":provider.model.id,"messages":input,"stream":true,"max_tokens":max_tokens});
            if !system.is_empty() {
                body["system"] = json!(system);
            }
            if !tools.is_empty() {
                body["tools"] = tools.iter().map(|t| json!({"name":t.name,"description":t.description,"input_schema":t.input_schema})).collect();
            }
            if let Some(effort) = effort.filter(|v| *v != "off") {
                if max_tokens < 2048 {
                    bail!("Anthropic thinking requires at least 2048 output tokens");
                }
                let budget = match effort {
                    "low" => 1024,
                    "medium" => 4096,
                    _ => 16384,
                }
                .min(max_tokens - 1);
                body["thinking"] = json!({"type":"enabled","budget_tokens":budget});
            }
            body
        }
    };
    if api == "anthropic-messages" && metadata["compat"]["supportsCacheControlOnTools"] != false {
        crate::performance::apply_prompt_cache(&mut body);
    }
    Ok(body)
}

fn bedrock(
    provider: &ResolvedProvider,
    messages: Vec<OpenAIMessage>,
    system: &str,
    tools: Vec<ToolDefinition>,
    max_tokens: u32,
) -> Result<Value> {
    let mut input = Vec::new();
    for message in messages {
        let mut blocks = Vec::new();
        if message.role == "tool" {
            blocks.push(json!({"toolResult":{"toolUseId":message.tool_call_id.context("Bedrock tool result has no call ID")?,"content":[{"text":text(&message.content)}]}}));
        } else if message
            .provider_content
            .as_ref()
            .is_some_and(|v| v["api"] == "bedrock-converse-stream")
        {
            blocks = message.provider_content.as_ref().unwrap()["items"]
                .as_array()
                .context("Invalid Bedrock native content")?
                .clone();
        } else {
            for part in parts(
                &message.content,
                "anthropic-messages",
                message.role == "assistant",
            )? {
                match part["type"].as_str() {
                    Some("text") if part["text"].as_str().is_some_and(|s| !s.is_empty()) => {
                        blocks.push(json!({"text":part["text"]}))
                    }
                    Some("image") => {
                        let mime = part["source"]["media_type"].as_str().unwrap_or_default();
                        let format = mime
                            .strip_prefix("image/")
                            .filter(|v| matches!(*v, "png" | "jpeg" | "gif" | "webp"))
                            .context("Bedrock supports PNG, JPEG, GIF, and WebP image inputs")?;
                        blocks.push(json!({"image":{"format":format,"source":{"bytes":part["source"]["data"]}}}));
                    }
                    _ => {}
                }
            }
            if let Some(calls) = message.tool_calls {
                for call in calls {
                    blocks.push(json!({"toolUse":{"toolUseId":call.id,"name":call.function.name,"input":serde_json::from_str::<Value>(&call.function.arguments)?}}));
                }
            }
        }
        let role = if message.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        if let Some(previous) = input.last_mut().filter(|v: &&mut Value| v["role"] == role) {
            previous["content"].as_array_mut().unwrap().extend(blocks);
        } else {
            input.push(json!({"role":role,"content":blocks}));
        }
    }
    let mut body = json!({"messages":input,"inferenceConfig":{"maxTokens":max_tokens}});
    if !system.is_empty() {
        body["system"] = json!([{"text":system}]);
    }
    if !tools.is_empty() {
        body["toolConfig"] = json!({"tools":tools.iter().map(|t|json!({"toolSpec":{"name":t.name,"description":t.description,"inputSchema":{"json":t.input_schema}}})).collect::<Vec<_>>()});
    }
    if let Some(effort) = provider.model.effort.as_deref().filter(|v| *v != "off") {
        if provider.model.id.contains("anthropic") || provider.model.id.contains("claude") {
            if max_tokens < 2048 {
                bail!("Bedrock Claude thinking requires at least 2048 output tokens");
            }
            body["additionalModelRequestFields"] = json!({"thinking":{"type":"enabled","budget_tokens":match effort {"low"=>1024,"medium"=>4096,_=>16384}.min(max_tokens-1)}});
        } else {
            bail!("Explicit reasoning effort is not implemented for this Bedrock model; omit --effort to use the model default");
        }
    }
    Ok(body)
}
