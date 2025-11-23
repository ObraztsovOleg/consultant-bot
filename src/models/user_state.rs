use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use teloxide::types::ChatId;

use super::{Booking, UserSession};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserState {
    pub current_model: String,
    pub current_session: Option<UserSession>,
    pub bookings: HashMap<String, Booking>,
    pub conversation_history: HashMap<ChatId, Vec<String>>,
    pub user_temperatures: HashMap<ChatId, f32>,
}