use serde::{Serialize, Deserialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TimeSlot {
    pub id: i32,
    pub duration_minutes: i32,
    pub description: String,
    pub is_active: bool,
    pub sort_order: i32,
}

impl TimeSlot {
    pub async fn get_all_active_slots(state: &crate::bot_state::BotState) -> Vec<Self> {
        match sqlx::query_as::<_, TimeSlot>(
            "SELECT id, duration_minutes, description, is_active, sort_order 
             FROM time_slots 
             WHERE is_active = true 
             ORDER BY sort_order ASC"
        )
        .fetch_all(&state.db.pool)
        .await {
            Ok(slots) => slots,
            Err(e) => {
                log::error!("Error fetching time slots from database: {}", e);
                // Fallback to default slots if DB fails
                vec![
                    TimeSlot {
                        id: 1,
                        duration_minutes: 30,
                        description: "Стандартная сессия".to_string(),
                        is_active: true,
                        sort_order: 1,
                    }
                ]
            }
        }
    }

    pub fn calculate_price(&self, price_per_minute: f64) -> f64 {
        price_per_minute * self.duration_minutes as f64
    }

    pub fn format_price(&self, price_per_minute: f64) -> String {
        let total_price = self.calculate_price(price_per_minute);
        format!("{} мин - {} Stars", self.duration_minutes, (total_price * 100.0) as i32)
    }
}
