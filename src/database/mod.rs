use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Database {
    pub pool: PgPool,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .min_connections(5)
            .acquire_timeout(Duration::from_secs(30))
            .idle_timeout(Duration::from_secs(300))
            .max_lifetime(Duration::from_secs(1800))
            .test_before_acquire(true)
            .connect(database_url)
            .await?;

        Ok(Database { pool })
    }

    pub async fn init(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_states (
                chat_id BIGINT PRIMARY KEY,
                current_model TEXT NOT NULL DEFAULT 'GigaChat-2-Max',
                current_session JSONB,
                bookings JSONB NOT NULL DEFAULT '{}',
                conversation_history JSONB NOT NULL DEFAULT '{}',
                user_temperatures JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_user_states_chat_id ON user_states (chat_id)"
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn health_check(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}