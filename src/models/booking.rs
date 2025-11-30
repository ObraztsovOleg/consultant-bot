use serde::{Serialize, Deserialize};
use teloxide::types::{ChatId, MessageId};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Booking {
    pub id: String,
    pub user_id: ChatId,
    pub consultant_model: String,
    pub duration_minutes: u32,
    pub total_price: f64,
    pub invoice_payload: String,
    pub is_paid: bool,
    pub is_completed: bool,
    pub created_at: DateTime<Utc>,
    pub payment_invoice_message_id: Option<MessageId>,
    pub expires_at: Option<DateTime<Utc>>,
}