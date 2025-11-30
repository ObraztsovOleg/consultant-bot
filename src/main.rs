use teloxide::{prelude::*, utils::command::BotCommands};
use std::env;
use std::time::Duration;
use tokio::time;

mod bot_state;
mod database;
mod llm;
mod models;
mod handlers;

use crate::bot_state::BotState;
use crate::database::Database;
use crate::models::payment_config::PaymentConfig;
use crate::handlers::{
    command_handler, message_handler, callback_handler, 
    pre_checkout_handler, successful_payment_handler
};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Доступные команды:")]
enum Command {
    #[command(description = "начать работу с ботом")]
    Start,
    #[command(description = "показать помощь")]
    Help,
    #[command(description = "выбрать консультанта")]
    Persona,
    #[command(description = "мои консультации")]
    MySessions,
    #[command(description = "список консультантов")] // Обновлено описание
    Settings,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Загружаем .env и инициализируем логирование
    dotenvy::dotenv().ok();
    env_logger::init();
    log::info!("Starting psychologist bot with PostgreSQL...");

    // Инициализация базы данных
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    let db = Database::new(&database_url).await?;
    db.init().await?;
    log::info!("✅ Database initialized");

    // Настройки оплаты Telegram Stars
    let payment_config = PaymentConfig {
        provider_token: None,
        currency: "XTR".to_string(), // Валюта для Telegram Stars
    };

    let state = BotState::new(db);

    // Фоновая задача для проверки сессий
    let state_clone = state.clone();
    tokio::spawn(async move {
        handlers::check_sessions_task(state_clone).await;
    });

    // Фоновая задача для очистки кэша
    let state_clone = state.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(600));
        loop {
            interval.tick().await;
            state_clone.cleanup_cache().await;
        }
    });

    let bot = Bot::from_env();

    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(command_handler)
        )
        .branch(
            Update::filter_message()
                .filter(|msg: Message| {
                    let has_payment = msg.successful_payment().is_some();
                    if has_payment {
                        log::info!("🎉 Payment detected in filter!");
                    }
                    has_payment
                })
                .endpoint(successful_payment_handler)
        )
        .branch(Update::filter_pre_checkout_query().endpoint(pre_checkout_handler))
        .branch(Update::filter_callback_query().endpoint(callback_handler))
        .branch(Update::filter_message().endpoint(message_handler));

    log::info!("🚀 Starting dispatcher with correct payment handling...");
    
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state, payment_config])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}