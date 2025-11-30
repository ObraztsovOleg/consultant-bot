use std::collections::HashMap;
use teloxide::types::{ChatId, MessageId};
use serde_json;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Instant, SystemTime};
use sqlx::Row;
use chrono::{DateTime, Utc};

use crate::models::{UserState, Booking, UserSession};
use crate::database::Database;

type UserCache = Arc<RwLock<HashMap<ChatId, (UserState, SystemTime)>>>;

#[derive(Clone)]
pub struct BotState {
    pub(crate) db: Database,
    cache: UserCache,
}

#[derive(Debug)]
pub enum BotStateError {
    DatabaseError(String),
    SerializationError(String),
    DataTooLarge(usize),
}

impl std::fmt::Display for BotStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BotStateError::DatabaseError(e) => write!(f, "Database error: {}", e),
            BotStateError::SerializationError(e) => write!(f, "Serialization error: {}", e),
            BotStateError::DataTooLarge(size) => write!(f, "Data too large: {} bytes", size),
        }
    }
}

impl std::error::Error for BotStateError {}

impl From<sqlx::Error> for BotStateError {
    fn from(err: sqlx::Error) -> Self {
        BotStateError::DatabaseError(err.to_string())
    }
}

impl From<serde_json::Error> for BotStateError {
    fn from(err: serde_json::Error) -> Self {
        BotStateError::SerializationError(err.to_string())
    }
}

impl BotState {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn save_user_state(&self, chat_id: ChatId, state: UserState) -> Result<(), BotStateError> {
        let start_time = Instant::now();

        let conversation_history_json = serde_json::to_value(&state.conversation_history)?;
        let user_temperatures_json = serde_json::to_value(&state.user_temperatures)?;
        let current_session_json = state.current_session
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;

        self.validate_data_size(&conversation_history_json, 5120)?;
        self.validate_data_size(&user_temperatures_json, 1024)?;

        sqlx::query(
            r#"
            INSERT INTO user_states 
            (chat_id, current_model, current_session, conversation_history, user_temperatures, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (chat_id) 
            DO UPDATE SET 
                current_model = EXCLUDED.current_model,
                current_session = EXCLUDED.current_session,
                conversation_history = EXCLUDED.conversation_history,
                user_temperatures = EXCLUDED.user_temperatures,
                updated_at = NOW()
            "#
        )
        .bind(chat_id.0 as i64)
        .bind(&state.current_model)
        .bind(current_session_json)
        .bind(conversation_history_json)
        .bind(user_temperatures_json)
        .execute(&self.db.pool)
        .await?;

        {
            let mut cache = self.cache.write().await;
            cache.insert(chat_id, (state, SystemTime::now()));
        }

        let duration = start_time.elapsed();
        log::debug!("💾 State saved for user {} in {:?}", chat_id, duration);

        Ok(())
    }

    pub async fn save_booking(&self, booking: &Booking) -> Result<(), BotStateError> {
        sqlx::query(
            r#"
            INSERT INTO bookings 
            (id, chat_id, consultant_model, duration_minutes, total_price, 
             invoice_payload, is_paid, is_completed, payment_invoice_message_id, 
             expires_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW() + INTERVAL '5 minutes', NOW())
            ON CONFLICT (id) 
            DO UPDATE SET 
                is_paid = EXCLUDED.is_paid,
                is_completed = EXCLUDED.is_completed,
                payment_invoice_message_id = EXCLUDED.payment_invoice_message_id,
                expires_at = CASE 
                    WHEN EXCLUDED.is_paid = true THEN NULL 
                    ELSE NOW() + INTERVAL '5 minutes' 
                END,
                updated_at = NOW()
            "#
        )
        .bind(&booking.id)
        .bind(booking.user_id.0 as i64)
        .bind(&booking.consultant_model)
        .bind(booking.duration_minutes as i32)
        .bind(booking.total_price)
        .bind(&booking.invoice_payload)
        .bind(booking.is_paid)
        .bind(booking.is_completed)
        .bind(booking.payment_invoice_message_id.map(|id| id.0 as i64))
        .execute(&self.db.pool)
        .await?;
    
        Ok(())
    }

    // Убрана функция is_time_slot_taken, так как больше нет бронирования по времени

    pub async fn get_user_bookings(&self, chat_id: ChatId) -> Result<Vec<Booking>, BotStateError> {
        // Сначала удаляем просроченные неоплаченные брони
        self.cleanup_expired_bookings().await?;

        let rows = sqlx::query(
            "SELECT id, chat_id, consultant_model, duration_minutes, total_price, 
                    invoice_payload, is_paid, is_completed, payment_invoice_message_id, 
                    created_at, expires_at
             FROM bookings 
             WHERE chat_id = $1 
             AND (is_paid = true OR expires_at > NOW())
             ORDER BY created_at DESC"
        )
        .bind(chat_id.0 as i64)
        .fetch_all(&self.db.pool)
        .await?;

        let mut bookings = Vec::new();
        for row in rows {
            let booking = Booking {
                id: row.get("id"),
                user_id: ChatId(row.get::<i64, _>("chat_id") as i64),
                consultant_model: row.get("consultant_model"),
                duration_minutes: row.get::<i32, _>("duration_minutes") as u32,
                total_price: row.get("total_price"),
                invoice_payload: row.get("invoice_payload"),
                is_paid: row.get("is_paid"),
                is_completed: row.get("is_completed"),
                payment_invoice_message_id: row.get::<Option<i64>, _>("payment_invoice_message_id")
                    .map(|id| MessageId(id as i32)),
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            };
            bookings.push(booking);
        }

        Ok(bookings)
    }

    pub async fn cleanup_expired_bookings(&self) -> Result<u64, BotStateError> {
        let result = sqlx::query(
            "DELETE FROM bookings 
             WHERE is_paid = false 
             AND expires_at <= NOW()"
        )
        .execute(&self.db.pool)
        .await?;

        let deleted_count = result.rows_affected();
        if deleted_count > 0 {
            log::info!("🧹 Cleaned up {} expired unpaid bookings", deleted_count);
        }

        Ok(deleted_count)
    }

    pub async fn get_booking_by_payload(&self, invoice_payload: &str) -> Result<Option<Booking>, BotStateError> {
        let row = sqlx::query(
            "SELECT id, chat_id, consultant_model, duration_minutes, total_price, 
                    invoice_payload, is_paid, is_completed, payment_invoice_message_id, 
                    created_at, expires_at
             FROM bookings 
             WHERE invoice_payload = $1"
        )
        .bind(invoice_payload)
        .fetch_optional(&self.db.pool)
        .await?;
    
        if let Some(row) = row {
            let booking = Booking {
                id: row.get("id"),
                user_id: ChatId(row.get::<i64, _>("chat_id") as i64),
                consultant_model: row.get("consultant_model"),
                duration_minutes: row.get::<i32, _>("duration_minutes") as u32,
                total_price: row.get("total_price"),
                invoice_payload: row.get("invoice_payload"),
                is_paid: row.get("is_paid"),
                is_completed: row.get("is_completed"),
                payment_invoice_message_id: row.get::<Option<i64>, _>("payment_invoice_message_id")
                    .map(|id| MessageId(id as i32)),
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            };
            Ok(Some(booking))
        } else {
            Ok(None)
        }
    }

    pub async fn get_booking_by_id(&self, booking_id: &str) -> Result<Option<Booking>, BotStateError> {
        let row = sqlx::query(
            "SELECT id, chat_id, consultant_model, duration_minutes, total_price, 
                    invoice_payload, is_paid, is_completed, payment_invoice_message_id, 
                    created_at, expires_at
             FROM bookings WHERE id = $1"
        )
        .bind(booking_id)
        .fetch_optional(&self.db.pool)
        .await?;

        if let Some(row) = row {
            let booking = Booking {
                id: row.get("id"),
                user_id: ChatId(row.get::<i64, _>("chat_id") as i64),
                consultant_model: row.get("consultant_model"),
                duration_minutes: row.get::<i32, _>("duration_minutes") as u32,
                total_price: row.get("total_price"),
                invoice_payload: row.get("invoice_payload"),
                is_paid: row.get("is_paid"),
                is_completed: row.get("is_completed"),
                payment_invoice_message_id: row.get::<Option<i64>, _>("payment_invoice_message_id")
                    .map(|id| MessageId(id as i32)),
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            };
            Ok(Some(booking))
        } else {
            Ok(None)
        }
    }

    pub async fn mark_booking_completed(&self, booking_id: &str) -> Result<(), BotStateError> {
        sqlx::query(
            "UPDATE bookings SET is_completed = true, updated_at = NOW() WHERE id = $1"
        )
        .bind(booking_id)
        .execute(&self.db.pool)
        .await?;
        
        log::info!("✅ Booking {} marked as completed", booking_id);
        Ok(())
    }

    pub async fn find_booking_for_session(&self, session: &UserSession) -> Result<Option<Booking>, BotStateError> {
        let row = sqlx::query(
            "SELECT id, chat_id, consultant_model, duration_minutes, total_price, 
                    invoice_payload, is_paid, is_completed, payment_invoice_message_id, 
                    created_at, expires_at
             FROM bookings 
             WHERE chat_id = $1 
             AND consultant_model = $2 
             AND is_paid = true
             ORDER BY created_at DESC
             LIMIT 1"
        )
        .bind(session.chat_id.0 as i64)
        .bind(&session.consultant_model)
        .fetch_optional(&self.db.pool)
        .await?;

        if let Some(row) = row {
            let booking = Booking {
                id: row.get("id"),
                user_id: ChatId(row.get::<i64, _>("chat_id") as i64),
                consultant_model: row.get("consultant_model"),
                duration_minutes: row.get::<i32, _>("duration_minutes") as u32,
                total_price: row.get("total_price"),
                invoice_payload: row.get("invoice_payload"),
                is_paid: row.get("is_paid"),
                is_completed: row.get("is_completed"),
                payment_invoice_message_id: row.get::<Option<i64>, _>("payment_invoice_message_id")
                    .map(|id| MessageId(id as i32)),
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            };
            Ok(Some(booking))
        } else {
            Ok(None)
        }
    }

    // Убрана функция get_booked_time_slots, так как больше нет бронирования по времени

    pub async fn get_consultant_price(&self, model: &str) -> Result<f64, BotStateError> {
        let row = sqlx::query(
            "SELECT price_per_minute FROM consultants WHERE model = $1 AND is_active = true"
        )
        .bind(model)
        .fetch_optional(&self.db.pool)
        .await?;

        if let Some(row) = row {
            Ok(row.get("price_per_minute"))
        } else {
            Ok(0.1) // Цена по умолчанию
        }
    }

    pub async fn get_user_state(&self, chat_id: ChatId) -> UserState {
        let start_time = Instant::now();

        {
            let cache = self.cache.read().await;
            if let Some((state, timestamp)) = cache.get(&chat_id) {
                if timestamp.elapsed().unwrap_or_default().as_secs() < 300 {
                    return state.clone();
                }
            }
        }

        match self.fetch_user_state_from_db(chat_id).await {
            Ok(state) => {
                let mut cache = self.cache.write().await;
                cache.insert(chat_id, (state.clone(), SystemTime::now()));

                let duration = start_time.elapsed();
                log::debug!("🎯 State loaded for user {} in {:?}", chat_id, duration);

                state
            }
            Err(e) => {
                log::error!("Error loading state for user {}: {}", chat_id, e);
                UserState::default()
            }
        }
    }

    async fn fetch_user_state_from_db(&self, chat_id: ChatId) -> Result<UserState, BotStateError> {
        let row = sqlx::query(
            "SELECT current_model, current_session, conversation_history, user_temperatures 
             FROM user_states WHERE chat_id = $1"
        )
        .bind(chat_id.0 as i64)
        .fetch_optional(&self.db.pool)
        .await?;

        if let Some(row) = row {
            let current_model: String = row.get("current_model");
            let current_session: Option<serde_json::Value> = row.get("current_session");
            let conversation_history_json: serde_json::Value = row.get("conversation_history");
            let user_temperatures_json: serde_json::Value = row.get("user_temperatures");

            let current_session = current_session
                .map(serde_json::from_value)
                .transpose()?;

            Ok(UserState {
                current_model,
                current_session,
                conversation_history: serde_json::from_value(conversation_history_json)?,
                user_temperatures: serde_json::from_value(user_temperatures_json)?,
                scheduled_time: None,
            })
        } else {
            Ok(UserState::default())
        }
    }

    pub async fn get_all_user_states(&self) -> HashMap<ChatId, UserState> {
        let mut states = HashMap::new();

        if let Ok(rows) = sqlx::query(
            "SELECT chat_id, current_model, current_session, conversation_history, user_temperatures 
             FROM user_states"
        )
        .fetch_all(&self.db.pool)
        .await {
            for row in rows {
                let chat_id = ChatId(row.get::<i64, _>("chat_id") as i64);
                let current_model: String = row.get("current_model");
                let current_session: Option<serde_json::Value> = row.get("current_session");
                let conversation_history_json: serde_json::Value = row.get("conversation_history");
                let user_temperatures_json: serde_json::Value = row.get("user_temperatures");

                if let (Ok(conversation_history), Ok(user_temperatures)) = (
                    serde_json::from_value(conversation_history_json),
                    serde_json::from_value(user_temperatures_json),
                ) {
                    let current_session = current_session
                        .map(serde_json::from_value)
                        .transpose()
                        .unwrap_or(None);

                    let user_state = UserState {
                        current_model,
                        current_session,
                        conversation_history,
                        user_temperatures,
                        scheduled_time: None,
                    };

                    states.insert(chat_id, user_state);
                }
            }
        }

        states
    }

    pub async fn cleanup_cache(&self) {
        let mut cache = self.cache.write().await;
        let now = SystemTime::now();
        let previous_count = cache.len();

        cache.retain(|_, (_, timestamp)| {
            now.duration_since(*timestamp).unwrap_or_default().as_secs() < 300
        });

        let current_count = cache.len();
        log::debug!("🧹 Cache cleaned: {} -> {} entries", previous_count, current_count);
    }

    fn validate_data_size(&self, data: &serde_json::Value, max_kb: usize) -> Result<(), BotStateError> {
        let size = serde_json::to_vec(data)?.len();
        if size > max_kb * 1024 {
            Err(BotStateError::DataTooLarge(size))
        } else {
            Ok(())
        }
    }

    // Новый метод для работы со слотами времени
    pub async fn get_time_slots(&self) -> Result<Vec<crate::models::TimeSlot>, BotStateError> {
        let rows = sqlx::query_as::<_, crate::models::TimeSlot>(
            "SELECT id, duration_minutes, description, is_active, sort_order 
             FROM time_slots 
             WHERE is_active = true 
             ORDER BY sort_order ASC"
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(rows)
    }
}