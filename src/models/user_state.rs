use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use teloxide::types::ChatId;
use chrono::{DateTime, Utc};

use super::UserSession;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserState {
    pub current_model: String,
    pub current_session: Option<UserSession>,
    pub conversation_history: HashMap<ChatId, Vec<String>>,
    pub user_temperatures: HashMap<ChatId, f32>,
    pub scheduled_time: Option<DateTime<Utc>>,
}