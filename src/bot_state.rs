use std::collections::HashMap;
use teloxide::types::ChatId;
use serde_json;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Instant, SystemTime};
use sqlx::Row;  // Важно для извлечения данных из строки БД

use crate::models::UserState;
use crate::database::Database;

type UserCache = Arc<RwLock<HashMap<ChatId, (UserState, SystemTime)>>>;

#[derive(Clone)]
pub struct BotState {
    db: Database,
    cache: UserCache,
}

// Простая ошибка без внешних зависимостей
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

        let bookings_json = serde_json::to_value(&state.bookings)?;
        let conversation_history_json = serde_json::to_value(&state.conversation_history)?;
        let user_temperatures_json = serde_json::to_value(&state.user_temperatures)?;
        let current_session_json = state.current_session
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;

        self.validate_data_size(&bookings_json, 1024)?;
        self.validate_data_size(&conversation_history_json, 5120)?;
        self.validate_data_size(&user_temperatures_json, 1024)?;

        sqlx::query(
            r#"
            INSERT INTO user_states 
            (chat_id, current_model, current_session, bookings, conversation_history, user_temperatures, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, NOW())
            ON CONFLICT (chat_id) 
            DO UPDATE SET 
                current_model = EXCLUDED.current_model,
                current_session = EXCLUDED.current_session,
                bookings = EXCLUDED.bookings,
                conversation_history = EXCLUDED.conversation_history,
                user_temperatures = EXCLUDED.user_temperatures,
                updated_at = NOW()
            "#
        )
        .bind(chat_id.0 as i64)
        .bind(&state.current_model)
        .bind(current_session_json)
        .bind(bookings_json)
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
            "SELECT current_model, current_session, bookings, conversation_history, user_temperatures 
             FROM user_states WHERE chat_id = $1"
        )
        .bind(chat_id.0 as i64)
        .fetch_optional(&self.db.pool)
        .await?;

        if let Some(row) = row {
            let current_model: String = row.get("current_model");
            let current_session: Option<serde_json::Value> = row.get("current_session");
            let bookings_json: serde_json::Value = row.get("bookings");
            let conversation_history_json: serde_json::Value = row.get("conversation_history");
            let user_temperatures_json: serde_json::Value = row.get("user_temperatures");

            let current_session = current_session
                .map(serde_json::from_value)
                .transpose()?;

            Ok(UserState {
                current_model,
                current_session,
                bookings: serde_json::from_value(bookings_json)?,
                conversation_history: serde_json::from_value(conversation_history_json)?,
                user_temperatures: serde_json::from_value(user_temperatures_json)?,
            })
        } else {
            Ok(UserState::default())
        }
    }

    pub async fn get_all_user_states(&self) -> HashMap<ChatId, UserState> {
        let mut states = HashMap::new();

        if let Ok(rows) = sqlx::query(
            "SELECT chat_id, current_model, current_session, bookings, conversation_history, user_temperatures 
             FROM user_states"
        )
        .fetch_all(&self.db.pool)
        .await {
            for row in rows {
                let chat_id = ChatId(row.get::<i64, _>("chat_id") as i64);
                let current_model: String = row.get("current_model");
                let current_session: Option<serde_json::Value> = row.get("current_session");
                let bookings_json: serde_json::Value = row.get("bookings");
                let conversation_history_json: serde_json::Value = row.get("conversation_history");
                let user_temperatures_json: serde_json::Value = row.get("user_temperatures");

                if let (Ok(bookings), Ok(conversation_history), Ok(user_temperatures)) = (
                    serde_json::from_value(bookings_json),
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
                        bookings,
                        conversation_history,
                        user_temperatures,
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
}
