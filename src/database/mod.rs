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
    
        // Удаляем старую таблицу bookings и создаем новую без scheduled_start
        sqlx::query("DROP TABLE IF EXISTS bookings")
            .execute(&self.pool)
            .await?;

        // Создаем новую таблицу bookings без scheduled_start
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bookings (
                id TEXT PRIMARY KEY,
                chat_id BIGINT NOT NULL,
                consultant_model TEXT NOT NULL,
                duration_minutes INTEGER NOT NULL,
                total_price DOUBLE PRECISION NOT NULL,
                invoice_payload TEXT NOT NULL,
                is_paid BOOLEAN NOT NULL DEFAULT false,
                is_completed BOOLEAN NOT NULL DEFAULT false,
                payment_invoice_message_id BIGINT,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                expires_at TIMESTAMP WITH TIME ZONE DEFAULT (NOW() + INTERVAL '5 minutes')
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
    
        // Таблица для консультантов
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS consultants (
                model TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL,
                specialty TEXT NOT NULL,
                greeting TEXT NOT NULL,
                prompt TEXT NOT NULL,
                price_per_minute DOUBLE PRECISION NOT NULL DEFAULT 0.1,
                is_active BOOLEAN NOT NULL DEFAULT true,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Таблица для слотов времени
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS time_slots (
                id SERIAL PRIMARY KEY,
                duration_minutes INTEGER NOT NULL,
                description TEXT NOT NULL,
                is_active BOOLEAN NOT NULL DEFAULT true,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Инициализация консультантов по умолчанию
        sqlx::query(
            r#"
            INSERT INTO consultants (model, name, description, specialty, greeting, prompt, price_per_minute) 
            VALUES 
                ('GigaChat-2-Max', 'Анна', 'Интерактивный помощник', 'Общение и поддержка в повседневных задачах', 
                 'Здравствуйте! Я Анна. Я помогу вам обсудить вопросы и получить полезные советы. Расскажите, что вас интересует?',
                 'Ты — Анна, виртуальный помощник, ориентированный на поддержку и советы в повседневной жизни. Твоя цель — помогать пользователю разбирать задачи, давать рекомендации и задавать уточняющие вопросы, чтобы пользователь самостоятельно находил решения.',
                 0.1),
                
                ('GigaChat-2-Pro', 'Максим', 'Наставник', 'Помощь в саморазвитии и планировании',
                 'Привет! Я Максим. Я помогу вам планировать задачи, развивать навыки и лучше понимать себя. С чего начнем?',
                 'Ты — Максим, виртуальный наставник для саморазвития. Твоя цель — помогать пользователю в постановке целей, планировании и развитии навыков. Ты задаешь наводящие вопросы и даешь советы, не навязывая решений.',
                 0.09),
                
                ('deepseek-chat', 'София', 'консультант', 'Поддержка и мотивация',
                 'Добрый день! Я София. Готова помочь обсудить идеи, задачи или получить мотивацию для новых целей.',
                 'Ты — София, виртуальный консультант для поддержки и мотивации. Твоя цель — создавать безопасное пространство для обсуждения идей и целей, помогать структурировать мысли и находить решения самостоятельно.',
                 0.08),
                
                ('GigaChat-2', 'Алексей', 'Коуч', 'Целеполагание и продуктивность',
                 'Здравствуйте! Я Алексей. Я помогу вам определить цели и разработать план действий. С чего начнем?',
                 'Ты — Алексей, виртуальный коуч по постановке целей и повышению продуктивности. Твоя цель — помогать пользователю выявлять задачи, строить планы и находить пути достижения целей. Ты даешь советы и задаешь уточняющие вопросы, чтобы пользователь сам находил оптимальные решения.',
                 0.07)
            ON CONFLICT (model) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                specialty = EXCLUDED.specialty,
                greeting = EXCLUDED.greeting,
                prompt = EXCLUDED.prompt,
                price_per_minute = EXCLUDED.price_per_minute,
                updated_at = NOW()
            "#
        )
        .execute(&self.pool)
        .await?;

        // Инициализация слотов времени по умолчанию
        // sqlx::query(
        //     r#"
        //     INSERT INTO time_slots (duration_minutes, description, sort_order) 
        //     VALUES 
        //         (15, 'Короткая сессия', 1),
        //         (30, 'Стандартная сессия', 2),
        //         (45, 'Продолжительная сессия', 3),
        //         (60, 'Расширенная сессия', 4)
        //     ON CONFLICT (id) DO UPDATE SET
        //         duration_minutes = EXCLUDED.duration_minutes,
        //         description = EXCLUDED.description,
        //         sort_order = EXCLUDED.sort_order,
        //         updated_at = NOW()
        //     "#
        // )
        // .execute(&self.pool)
        // .await?;
    
        // Создаем индексы
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
            "CREATE INDEX IF NOT EXISTS idx_bookings_expires_at ON bookings (expires_at)"
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_bookings_invoice_payload ON bookings (invoice_payload)"
        )
        .execute(&self.pool)
        .await?;
    
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_consultants_model ON consultants (model)"
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_consultants_active ON consultants (is_active)"
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_time_slots_active ON time_slots (is_active)"
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_time_slots_order ON time_slots (sort_order)"
        )
        .execute(&self.pool)
        .await?;
    
        Ok(())
    }
}