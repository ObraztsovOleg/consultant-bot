use serde::{Serialize, Deserialize};
use sqlx::FromRow;

use crate::bot_state::BotState;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AIAssistant {
    pub id: i32, // Новое поле ID
    pub name: String,
    pub prompt: String,
    pub model: String,
    pub description: String,
    pub specialty: String,
    pub greeting: String,
    pub price_per_minute: f64,
}

impl AIAssistant {
    pub async fn get_all_assistants(state: &BotState) -> Vec<Self> {
        match sqlx::query_as::<_, AIAssistant>(
            "SELECT id, name, prompt, model, description, specialty, greeting, price_per_minute 
             FROM consultants 
             WHERE is_active = true 
             ORDER BY price_per_minute DESC"
        )
        .fetch_all(&state.db.pool)
        .await {
            Ok(assistants) => assistants,
            Err(e) => {
                log::error!("Error fetching assistants from database: {}", e);
                // Fallback to default assistants if DB fails
                vec![
                    AIAssistant {
                        id: 1,
                        name: "Анна".to_string(),
                        model: "GigaChat-2-Max".to_string(),
                        description: "Интерактивный помощник".to_string(),
                        specialty: "Общение и поддержка в повседневных задачах".to_string(),
                        greeting: "Здравствуйте! Я Анна. Я помогу вам обсудить вопросы и получить полезные советы. Расскажите, что вас интересует?".to_string(),
                        price_per_minute: 0.1,
                        prompt: "Ты — Анна, виртуальный помощник, ориентированный на поддержку и советы в повседневной жизни. Твоя цель — помогать пользователю разбирать задачи, давать рекомендации и задавать уточняющие вопросы, чтобы пользователь самостоятельно находил решения.".to_string(),
                    }
                ]
            }
        }
    }

    pub async fn find_by_id_with_price(state: &BotState, id: i32) -> Option<Self> {
        match sqlx::query_as::<_, AIAssistant>(
            "SELECT id, name, prompt, model, description, specialty, greeting, price_per_minute 
             FROM consultants 
             WHERE id = $1 AND is_active = true"
        )
        .bind(id)
        .fetch_optional(&state.db.pool)
        .await {
            Ok(Some(assistant)) => Some(assistant),
            Ok(None) => {
                log::warn!("Assistant with id {} not found in database", id);
                None
            }
            Err(e) => {
                log::error!("Error fetching assistant from database: {}", e);
                None
            }
        }
    }

    pub async fn find_by_model_with_price(state: &BotState, model: &str) -> Option<Self> {
        match sqlx::query_as::<_, AIAssistant>(
            "SELECT id, name, prompt, model, description, specialty, greeting, price_per_minute 
             FROM consultants 
             WHERE model = $1 AND is_active = true"
        )
        .bind(model)
        .fetch_optional(&state.db.pool)
        .await {
            Ok(Some(assistant)) => Some(assistant),
            Ok(None) => {
                log::warn!("Assistant with model {} not found in database", model);
                None
            }
            Err(e) => {
                log::error!("Error fetching assistant from database: {}", e);
                None
            }
        }
    }

    pub async fn find_by_model(state: &BotState, model: &str) -> Option<Self> {
        Self::find_by_model_with_price(state, model).await
    }

    pub async fn find_by_id(state: &BotState, id: i32) -> Option<Self> {
        Self::find_by_id_with_price(state, id).await
    }

    pub fn calculate_price(&self, duration_minutes: u32) -> (f64, u32) {
        let price_ton = self.price_per_minute * duration_minutes as f64;
        let price_nanoton = (price_ton * 1_000_000_000.0) as u32;
        (price_ton, price_nanoton)
    }

    // Новый метод для административных задач
    pub async fn update_assistant(state: &BotState, assistant: &AIAssistant) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        sqlx::query(
            r#"
            INSERT INTO consultants (id, model, name, description, specialty, greeting, prompt, price_per_minute)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                model = EXCLUDED.model,
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                specialty = EXCLUDED.specialty,
                greeting = EXCLUDED.greeting,
                prompt = EXCLUDED.prompt,
                price_per_minute = EXCLUDED.price_per_minute,
                updated_at = NOW()
            "#
        )
        .bind(assistant.id)
        .bind(&assistant.model)
        .bind(&assistant.name)
        .bind(&assistant.description)
        .bind(&assistant.specialty)
        .bind(&assistant.greeting)
        .bind(&assistant.prompt)
        .bind(assistant.price_per_minute)
        .execute(&state.db.pool)
        .await?;

        Ok(())
    }

    // Метод для деактивации консультанта
    pub async fn deactivate_assistant(state: &BotState, id: i32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        sqlx::query(
            "UPDATE consultants SET is_active = false, updated_at = NOW() WHERE id = $1"
        )
        .bind(id)
        .execute(&state.db.pool)
        .await?;

        Ok(())
    }
}