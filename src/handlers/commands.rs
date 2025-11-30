use teloxide::types::ParseMode;
use teloxide::{prelude::*};
use std::error::Error;

use crate::bot_state::BotState;
use crate::models::AIAssistant;
use crate::handlers::utils::{
    main_menu_keyboard,
    make_ai_keyboard, make_consultants_info_keyboard, show_user_sessions
};

use crate::Command;

pub async fn command_handler(
    bot: Bot,
    msg: Message,
    cmd: Command,
    state: BotState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match cmd {
        Command::Start => handle_start(bot, msg, state).await?,
        Command::Help => handle_help(bot, msg).await?,
        Command::Persona => handle_persona(bot, msg, state).await?,
        Command::MySessions => handle_my_sessions(bot, msg, state).await?,
        Command::Settings => handle_consultants_list(bot, msg, state).await?, // Изменено на список консультантов
    }
    Ok(())
}

async fn handle_start(
    bot: Bot,
    msg: Message,
    state: BotState
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let user_state = state.get_user_state(msg.chat.id).await;
    let assistants = AIAssistant::get_all_assistants(&state).await;
    let _current_assistant = AIAssistant::find_by_model(&state, &user_state.current_model).await
        .unwrap_or_else(|| {
            // Fallback если не найден в БД
            assistants.first()
                .cloned()
                .unwrap_or_else(|| AIAssistant {
                    name: "Анна".to_string(),
                    model: "GigaChat-2-Max".to_string(),
                    description: "Интерактивный помощник".to_string(),
                    specialty: "Общение и поддержка".to_string(),
                    greeting: "Здравствуйте!".to_string(),
                    prompt: "Ты помощник.".to_string(),
                    price_per_minute: 0.1,
                })
        });

    let start_text = "👋 *Добро пожаловать в ListenerBot\\!*\n\n\
        🧠 *Кто я?*\n\
        Я — ИИ\\-ассистент для эмоциональной поддержки\\.\n\
        Я не являюсь психологом, психотерапевтом или медицинским специалистом\\.\n\n\
        📋 *Команды:*\n\
        /start – начать работу\n\
        /persona – выбрать консультанта \\(стиль общения\\)\n\
        /mysessions – ваши оплаченные сессии\n\
        /settings – список консультантов\n\n\
        🛠️ *Как это работает:*\n\
        1\\. Выберите консультанта \\(стиль общения\\)\n\
        2\\. Оплатите время общения через Telegram Stars\n\
        3\\. Общайтесь с ИИ в течение оплаченного времени\n\
        4\\. Можно продлевать сессию\n\n\
        🔐 *Конфиденциальность:*\n\
        • Сообщения не передаются третьим лицам\n\
        • Анонимность\n\
        • Никаких реальных специалистов в проекте нет\n\n\
        ⚠️ *Важно:*\n\
        Ответы носят информационный и поддерживающий характер и не заменяют профессиональную помощь\\.";

    bot.send_message(msg.chat.id, start_text)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(main_menu_keyboard())
        .await?;

    Ok(())
}

async fn handle_help(
    bot: Bot,
    msg: Message
) -> Result<(), Box<dyn Error + Send + Sync>> {
    bot.send_message(
        msg.chat.id,
        "🫂 *Помощь по боту*\n\n\
        /start - начать работу\n\
        /persona - выбрать консультанта\n\
        /mysessions - мои сессии\n\
        /settings - список консультантов\n\n\
        *Как это работает:*\n\
        1\\. Выберите консультанта\n\
        2\\. Оплатите время через Telegram Stars\n\
        3\\. Общайтесь с ИИ в течение оплаченного времени\n\
        4\\. Можно продлить при необходимости\n\n\
        ⚠️ Ответы носят информационный характер и не являются консультацией специалиста\\."
    )
    .parse_mode(ParseMode::MarkdownV2)
    .await?;

    Ok(())
}

async fn handle_persona(
    bot: Bot,
    msg: Message,
    state: BotState
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let keyboard = make_ai_keyboard(&state).await;

    bot.send_message(
        msg.chat.id,
        "👥 *Выберите консультанта*\n\n\
Каждый консультант — это стиль общения ИИ с разным характером и ценой\\.\n\
Это не психологи и не специалисты\\."
    )
    .parse_mode(ParseMode::MarkdownV2)
    .reply_markup(keyboard)
    .await?;

    Ok(())
}

async fn handle_my_sessions(
    bot: Bot,
    msg: Message,
    state: BotState
) -> Result<(), Box<dyn Error + Send + Sync>> {
    show_user_sessions(&bot, msg.chat.id, &state).await?;
    Ok(())
}

// Новая функция для отображения списка консультантов с информацией
async fn handle_consultants_list(
    bot: Bot,
    msg: Message,
    state: BotState
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let keyboard = make_consultants_info_keyboard(&state).await;

    bot.send_message(
        msg.chat.id,
        "👥 *Список консультантов*\n\n\
Выберите консультанта чтобы увидеть подробную информацию:\n\n\
Каждый консультант — это стиль общения ИИ с разным характером и ценой\\.\n\
Это не психологи и не специалисты\\."
    )
    .parse_mode(ParseMode::MarkdownV2)
    .reply_markup(keyboard)
    .await?;

    Ok(())
}