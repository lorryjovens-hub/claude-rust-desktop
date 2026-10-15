//! SendMessage Tool - Inter-agent messaging
//!
//! Provides messaging capability between agents.

use crate::tools::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub from: String,
    pub to: String,
    pub content: String,
    pub timestamp: u64,
    pub read: bool,
}

impl Message {
    pub fn new(from: String, to: String, content: String) -> Self {
        Self {
            id: format!("msg_{}", SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()),
            from,
            to,
            content,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            read: false,
        }
    }
}

pub struct MessageStore {
    messages: Vec<Message>,
    pending: HashMap<String, Vec<Message>>,
}

impl Default for MessageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageStore {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            pending: HashMap::new(),
        }
    }

    pub fn send_message(&mut self, from: String, to: String, content: String) -> Message {
        let msg = Message::new(from.clone(), to.clone(), content);
        self.pending.entry(to.clone()).or_default().push(msg.clone());
        self.messages.push(msg.clone());
        msg
    }

    pub fn get_messages(&self, recipient: &str, unread_only: bool) -> Vec<&Message> {
        self.messages.iter()
            .filter(|m| m.to == recipient && (!unread_only || !m.read))
            .collect()
    }

    pub fn mark_read(&mut self, message_id: &str) -> bool {
        if let Some(msg) = self.messages.iter_mut().find(|m| m.id == message_id) {
            msg.read = true;
            return true;
        }
        false
    }

    pub fn get_pending_count(&self, recipient: &str) -> usize {
        self.pending.get(recipient).map(|v| v.len()).unwrap_or(0)
    }
}

pub type SharedMessageStore = Arc<RwLock<MessageStore>>;

pub fn create_shared_message_store() -> SharedMessageStore {
    Arc::new(RwLock::new(MessageStore::new()))
}

static MESSAGE_STORE: std::sync::OnceLock<SharedMessageStore> = std::sync::OnceLock::new();

pub fn get_message_store() -> SharedMessageStore {
    MESSAGE_STORE.get_or_init(create_shared_message_store).clone()
}

#[derive(Debug, Clone)]
pub struct SendMessageTool;

impl SendMessageTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SendMessageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct SendMessageInput {
    to: String,
    content: String,
    #[serde(default)]
    from: String,
}

#[derive(Debug, Deserialize)]
struct GetMessagesInput {
    #[serde(default = "default_recipient")]
    recipient: String,
    #[serde(default)]
    unread_only: bool,
}

fn default_recipient() -> String {
    "agent".to_string()
}

#[derive(Debug, Deserialize)]
struct MarkReadInput {
    message_id: String,
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "SendMessage"
    }

    fn description(&self) -> &str {
        "Send a message to another agent or get pending messages. Operations: send, receive, pending_count"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["send", "receive", "pending_count", "mark_read"],
                    "description": "Operation to perform"
                },
                "to": {
                    "type": "string",
                    "description": "Recipient agent ID"
                },
                "content": {
                    "type": "string",
                    "description": "Message content"
                },
                "from": {
                    "type": "string",
                    "description": "Sender ID (default: agent)"
                },
                "recipient": {
                    "type": "string",
                    "description": "Recipient to check messages for"
                },
                "unread_only": {
                    "type": "boolean",
                    "description": "Only return unread messages"
                },
                "message_id": {
                    "type": "string",
                    "description": "Message ID to mark as read"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        #[derive(Debug, Deserialize)]
        struct MessageInput {
            operation: String,
            #[serde(default)]
            to: Option<String>,
            #[serde(default)]
            content: Option<String>,
            #[serde(default)]
            from: Option<String>,
            #[serde(default = "default_recipient")]
            recipient: String,
            #[serde(default)]
            unread_only: Option<bool>,
            #[serde(default)]
            message_id: Option<String>,
        }

        let input: MessageInput = serde_json::from_value(input)
            .map_err(|e| ToolError {
                message: format!("Invalid input: {}", e),
                code: Some("invalid_input".to_string()),
            })?;

        match input.operation.as_str() {
            "send" => {
                let to = input.to.ok_or_else(|| ToolError {
                    message: "Recipient 'to' is required for send operation".to_string(),
                    code: Some("missing_field".to_string()),
                })?;
                let content = input.content.ok_or_else(|| ToolError {
                    message: "Message 'content' is required for send operation".to_string(),
                    code: Some("missing_field".to_string()),
                })?;
                let from = input.from.unwrap_or_else(|| "agent".to_string());

                let store = get_message_store();
                let mut store = store.write().await;
                let msg = store.send_message(from, to, content);

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&msg).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "receive" => {
                let unread_only = input.unread_only.unwrap_or(false);
                let store = get_message_store();
                let store = store.read().await;
                let messages = store.get_messages(&input.recipient, unread_only);

                let message_summaries: Vec<serde_json::Value> = messages.iter().map(|m| {
                    serde_json::json!({
                        "id": m.id,
                        "from": m.from,
                        "content": m.content,
                        "timestamp": m.timestamp,
                        "read": m.read,
                    })
                }).collect();

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&message_summaries).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "pending_count" => {
                let store = get_message_store();
                let store = store.read().await;
                let count = store.get_pending_count(&input.recipient);

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&serde_json::json!({
                        "recipient": input.recipient,
                        "pending_count": count
                    })).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            "mark_read" => {
                let message_id = input.message_id.ok_or_else(|| ToolError {
                    message: "Message 'message_id' is required for mark_read operation".to_string(),
                    code: Some("missing_field".to_string()),
                })?;

                let store = get_message_store();
                let mut store = store.write().await;
                let success = store.mark_read(&message_id);

                Ok(ToolOutput {
                    output_type: "json".to_string(),
                    content: serde_json::to_string_pretty(&serde_json::json!({
                        "success": success,
                        "message_id": message_id
                    })).unwrap(),
                    metadata: HashMap::new(),
                })
            }
            _ => Err(ToolError {
                message: format!("Unknown operation: {}", input.operation),
                code: Some("unknown_operation".to_string()),
            }),
        }
    }
}