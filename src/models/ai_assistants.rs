use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIAssistant {
    pub name: String,
    pub prompt: String,
    pub model: String,
    pub description: String,
    pub specialty: String,
    pub greeting: String,
    pub price_per_minute: f64,
}

impl AIAssistant {
    pub fn get_all_assistants() -> Vec<Self> {
        vec![
            AIAssistant {
                name: "Анна".to_string(),
                model: "GigaChat-2-Max".to_string(),
                description: "Интерактивный помощник".to_string(),
                specialty: "Общение и поддержка в повседневных задачах".to_string(),
                greeting: "Здравствуйте\\! Я Анна\\. Я помогу вам обсудить вопросы и получить полезные советы\\. Расскажите, что вас интересует?".to_string(),
                price_per_minute: 0.1, // Будет переопределено из базы
                prompt: "Ты \\— Анна, виртуальный помощник, ориентированный на поддержку и советы в повседневной жизни\\. \
                          Твоя цель \\— помогать пользователю разбирать задачи, давать рекомендации и задавать уточняющие вопросы, \
                          чтобы пользователь самостоятельно находил решения\\.".to_string(),
            },
            AIAssistant {
                name: "Максим".to_string(),
                model: "GigaChat-2-Pro".to_string(),
                description: "Наставник".to_string(),
                specialty: "Помощь в саморазвитии и планировании".to_string(),
                greeting: "Привет\\! Я Максим\\. Я помогу вам планировать задачи, развивать навыки и лучше понимать себя\\. С чего начнем?".to_string(),
                price_per_minute: 0.09, // Будет переопределено из базы
                prompt: "Ты \\— Максим, виртуальный наставник для саморазвития\\. \
                          Твоя цель \\— помогать пользователю в постановке целей, планировании и развитии навыков\\. \
                          Ты задаешь наводящие вопросы и даешь советы, не навязывая решений\\.".to_string(),
            },
            AIAssistant {
                name: "София".to_string(),
                model: "deepseek-chat".to_string(),
                description: "консультант".to_string(),
                specialty: "Поддержка и мотивация".to_string(),
                greeting: "Добрый день\\! Я София\\. Готова помочь обсудить идеи, задачи или получить мотивацию для новых целей\\.".to_string(),
                price_per_minute: 0.08, // Будет переопределено из базы
                prompt: "Ты \\— София, виртуальный консультант для поддержки и мотивации\\. \
                          Твоя цель \\— создавать безопасное пространство для обсуждения идей и целей, помогать структурировать мысли и находить решения самостоятельно\\.".to_string(),
            },
            AIAssistant {
                name: "Алексей".to_string(),
                model: "GigaChat-2".to_string(),
                description: "Коуч".to_string(),
                specialty: "Целеполагание и продуктивность".to_string(),
                greeting: "Здравствуйте\\! Я Алексей\\. Я помогу вам определить цели и разработать план действий\\. С чего начнем?".to_string(),
                price_per_minute: 0.07, // Будет переопределено из базы
                prompt: "Ты \\— Алексей, виртуальный коуч по постановке целей и повышению продуктивности\\. \
                          Твоя цель \\— помогать пользователю выявлять задачи, строить планы и находить пути достижения целей\\. \
                          Ты даешь советы и задаешь уточняющие вопросы, чтобы пользователь сам находил оптимальные решения\\.".to_string(),
            },
        ]
    }

    pub async fn find_by_model_with_price(state: &crate::bot_state::BotState, model: &str) -> Option<Self> {
        let mut assistant = Self::get_all_assistants()
            .into_iter()
            .find(|assistant| assistant.model == model)?;

        // Получаем актуальную цену из базы данных
        if let Ok(price) = state.get_psychologist_price(model).await {
            assistant.price_per_minute = price;
        }

        Some(assistant)
    }

    pub fn find_by_model(model: &str) -> Option<Self> {
        Self::get_all_assistants()
            .into_iter()
            .find(|assistant| assistant.model == model)
    }

    pub fn calculate_price(&self, duration_minutes: u32) -> (f64, u32) {
        let price_ton = self.price_per_minute * duration_minutes as f64;
        let price_nanoton = (price_ton * 1_000_000_000.0) as u32;
        (price_ton, price_nanoton)
    }
}