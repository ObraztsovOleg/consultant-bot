use serde::{Serialize, Deserialize};
use teloxide::types::ChatId;
use chrono::{DateTime, Utc};

use crate::llm::config::ChatMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub chat_id: ChatId,
    pub psychologist_model: String,
    pub session_start: DateTime<Utc>,
    pub paid_until: DateTime<Utc>,
    pub total_price: f64,
    pub messages_exchanged: u32,
    pub history: Vec<ChatMessage>,
    pub is_active: bool,
    pub scheduled_start: Option<DateTime<Utc>>, // Добавляем поле для запланированного времени
}