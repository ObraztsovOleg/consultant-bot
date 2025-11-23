use teloxide::types::ParseMode;
use teloxide::{prelude::*};
use std::error::Error;

use crate::bot_state::BotState;
use crate::models::AIAssistant;
use crate::handlers::utils::{
    escape_markdown_v2, format_float, main_menu_keyboard,
    make_ai_keyboard, make_settings_keyboard,
    get_user_temperature, show_user_sessions
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
        Command::Persona => handle_persona(bot, msg).await?,
        Command::MySessions => handle_my_sessions(bot, msg, state).await?,
        Command::Settings => handle_settings(bot, msg, state).await?,
    }
    Ok(())
}

async fn handle_start(
    bot: Bot,
    msg: Message,
    state: BotState
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let user_state = state.get_user_state(msg.chat.id).await;
    let _current_assistant = AIAssistant::find_by_model(&user_state.current_model)
        .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());

    let start_text = "👋 *Добро пожаловать в ListenerBot\\!*\n\n\
        🧠 *Кто я?*\n\
        Я — ИИ\\-ассистент для эмоциональной поддержки\\.\n\
        Я не являюсь психологом, психотерапевтом или медицинским специалистом\\.\n\n\
        📋 *Команды:*\n\
        /start – начать работу\n\
        /persona – выбрать консультанта \\(стиль общения\\)\n\
        /mysessions – ваши оплаченные сессии\n\
        /settings – настройки стиля общения\n\n\
        🛠️ *Как это работает:*\n\
        1\\. Выберите консультанта \\(стиль общения\\)\n\
        2\\. Оплатите время общения \\(USDT / BTC\\)\n\
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
        /settings - настройки\n\n\
        *Как это работает:*\n\
        1\\. Выберите консультанта\n\
        2\\. Оплатите время (USDT/BTC)\n\
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
    msg: Message
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let keyboard = make_ai_keyboard();

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

async fn handle_settings(
    bot: Bot,
    msg: Message,
    state: BotState
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let user_state = state.get_user_state(msg.chat.id).await;
    let current_assistant = AIAssistant::find_by_model(&user_state.current_model)
        .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
    let temp = get_user_temperature(msg.chat.id, &state).await;

    bot.send_message(
        msg.chat.id,
        format!(
            "⚙️ *Настройки:*\n\n\
            *Консультант:* {}\n\
            *Характер стиля:* {}\n\
            *Цена:* {} USD/мин\n\
            *Эмпатия \\(температура\\):* {}\n\n\
            Температура влияет на вариативность и теплоту ответов ИИ\\.",
            escape_markdown_v2(&current_assistant.name),
            escape_markdown_v2(&current_assistant.specialty),
            format_float(current_assistant.price_per_minute),
            format_float(temp as f64)
        ),
    )
    .parse_mode(ParseMode::MarkdownV2)
    .reply_markup(make_settings_keyboard())
    .await?;

    Ok(())
}
