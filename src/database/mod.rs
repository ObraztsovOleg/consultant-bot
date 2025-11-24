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
        // Таблица user_states
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_states (
                chat_id BIGINT PRIMARY KEY,
                current_model TEXT NOT NULL DEFAULT 'GigaChat-2-Max',
                current_session JSONB,
                conversation_history JSONB NOT NULL DEFAULT '{}',
                user_temperatures JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
    
        // Отдельная таблица для bookings
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bookings (
                id TEXT PRIMARY KEY,
                chat_id BIGINT NOT NULL,
                psychologist_model TEXT NOT NULL,
                duration_minutes INTEGER NOT NULL,
                total_price DOUBLE PRECISION NOT NULL,
                ton_invoice_payload TEXT NOT NULL,
                is_paid BOOLEAN NOT NULL DEFAULT false,
                is_completed BOOLEAN NOT NULL DEFAULT false,
                payment_invoice_message_id BIGINT,
                scheduled_start TIMESTAMP WITH TIME ZONE,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                expires_at TIMESTAMP WITH TIME ZONE DEFAULT (NOW() + INTERVAL '5 minutes')
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
    
        // Таблица для цен консультантов в TON
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS psychologist_prices (
                model TEXT PRIMARY KEY,
                price_per_minute_ton DOUBLE PRECISION NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
    
        // Инициализация цен по умолчанию
        sqlx::query(
            r#"
            INSERT INTO psychologist_prices (model, price_per_minute_ton) 
            VALUES 
                ('GigaChat-2-Max', 0.1),
                ('GigaChat-2-Pro', 0.09),
                ('deepseek-chat', 0.08),
                ('GigaChat-2', 0.07)
            ON CONFLICT (model) DO NOTHING
            "#
        )
        .execute(&self.pool)
        .await?;
    
        // Создаем индексы (БЕЗ условий с NOW())
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_user_states_chat_id ON user_states (chat_id)"
        )
        .execute(&self.pool)
        .await?;
    
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_bookings_chat_id ON bookings (chat_id)"
        )
        .execute(&self.pool)
        .await?;
    
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_bookings_is_paid ON bookings (is_paid)"
        )
        .execute(&self.pool)
        .await?;
    
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_bookings_scheduled_start ON bookings (scheduled_start)"
        )
        .execute(&self.pool)
        .await?;
    
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_bookings_expires_at ON bookings (expires_at)"
        )
        .execute(&self.pool)
        .await?;
    
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_bookings_model_time ON bookings (psychologist_model, scheduled_start)"
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