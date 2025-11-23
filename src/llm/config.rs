use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceChatRequest {
    pub provider: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default = "default_temperature")]
    pub temperature: f32
}
fn default_temperature() -> f32 { 0.1 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceEmbeddingRequest {
    pub provider: String,
    pub model: String,
    pub input: String
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceChatResponse {
    pub content: Option<String>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServiceEmbeddingResponse {
    pub content: Vec<f32>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub function: FunctionCall,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}
