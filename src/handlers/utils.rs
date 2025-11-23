use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup, ParseMode, ReplyMarkup};
use std::collections::HashMap;

use crate::bot_state::BotState;
use crate::models::{AIAssistant, Booking};
use chrono::Utc;

/// Экранирование MarkdownV2
pub fn escape_markdown_v2(text: &str) -> String {
    let specials = ['_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!'];
    let mut out = String::with_capacity(text.len() * 2);
    
    for ch in text.chars() {
        if specials.contains(&ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Формат числа
pub fn format_float(price: f64) -> String {
    let formatted = format!("{:.2}", price);
    escape_markdown_v2(&formatted)
}

/// Формат информации об AI-персоне
pub fn format_ai_info(assistant: &AIAssistant) -> String {
    format!(
        "{} - {} USD/мин",
        escape_markdown_v2(&assistant.name),
        assistant.price_per_minute
    )
}

/// Главное меню
pub fn main_menu_keyboard() -> ReplyMarkup {
    ReplyMarkup::Keyboard(
        KeyboardMarkup::new(vec![
            vec![KeyboardButton::new("👥 Выбрать консультанта")],
            vec![KeyboardButton::new("💰 Мои сессии")],
            vec![KeyboardButton::new("⚙️ Настройки"), KeyboardButton::new("ℹ️ О боте")],
        ])
        .resize_keyboard()
        .one_time_keyboard()
    )
}

/// Клавиатура выбора AI-персоны
pub fn make_ai_keyboard() -> InlineKeyboardMarkup {
    let assistants = AIAssistant::get_all_assistants();
    let mut keyboard = Vec::new();

    for assistant in assistants {
        keyboard.push(vec![InlineKeyboardButton::callback(
            format_ai_info(&assistant),
            format!("select_ai_{}", assistant.model),
        )]);
    }

    keyboard.push(vec![InlineKeyboardButton::callback("❌ Отмена", "cancel_selection")]);

    InlineKeyboardMarkup::new(keyboard)
}

/// Клавиатура выбора продолжительности сессии
pub fn make_booking_keyboard(assistant: &AIAssistant) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(
                format!("30 мин - {:.8} BTC", assistant.calculate_price_btc(30, 45000.0).0),
                format!("book_{}_30", assistant.model)
            ),
            InlineKeyboardButton::callback(
                format!("60 мин - {:.8} BTC", assistant.calculate_price_btc(60, 45000.0).0),
                format!("book_{}_60", assistant.model)
            ),
        ],
        vec![
            InlineKeyboardButton::callback(
                format!("15 мин - {:.8} BTC", assistant.calculate_price_btc(15, 45000.0).0),
                format!("book_{}_15", assistant.model)
            ),
            InlineKeyboardButton::callback(
                format!("45 мин - {:.8} BTC", assistant.calculate_price_btc(45, 45000.0).0),
                format!("book_{}_45", assistant.model)
            ),
        ],
        vec![InlineKeyboardButton::callback("❌ Отмена", "cancel_selection")],
    ])
}

/// Настройки сессии
pub fn make_settings_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("📈 Низкая (0.1)", "temp_0.1"),
            InlineKeyboardButton::callback("🌡️ Средняя (0.3)", "temp_0.3"),
            InlineKeyboardButton::callback("🔥 Высокая (0.7)", "temp_0.7"),
        ],
        vec![InlineKeyboardButton::callback("👥 Сменить консультанта", "change_ai")],
        vec![InlineKeyboardButton::callback("🗑️ Очистить историю", "clear_history")],
    ])
}

/// Клавиатура управления сессией
pub fn make_session_management_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("⏱️ Продлить", "extend_session"),
            InlineKeyboardButton::callback("⏹️ Завершить", "end_session"),
        ],
        vec![InlineKeyboardButton::callback("📋 Новое бронирование", "new_booking")],
    ])
}

/// Получить температуру/креативность пользователя
pub async fn get_user_temperature(chat_id: ChatId, state: &BotState) -> f32 {
    let user_state = state.get_user_state(chat_id).await;
    user_state.user_temperatures.get(&chat_id).copied().unwrap_or(0.3)
}

/// Показать текущие сессии
pub async fn show_user_sessions(bot: &Bot, chat_id: ChatId, state: &BotState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let user_state = state.get_user_state(chat_id).await;
    
    if let Some(session) = user_state.current_session {
        let remaining_time = if session.is_active && Utc::now() < session.paid_until {
            let remaining = session.paid_until - Utc::now();
            format!("{} мин {} сек", remaining.num_minutes(), remaining.num_seconds() % 60)
        } else {
            "Завершена".to_string()
        };
        
        let assistant = AIAssistant::find_by_model(&session.psychologist_model)
            .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
            
        bot.send_message(
            chat_id,
            format!(
                "💰 *Ваши сессии*\n\n\
                *Текущая сессия:*\n\
                • Консультант: {}\n\
                • Сообщений: {}\n\
                • Потрачено: {:.8} BTC\n\
                • Осталось времени: {}\n\
                • Статус: {}\n\n\
                *Ближайшие бронирования:*\n{}",
                escape_markdown_v2(&assistant.name),
                session.messages_exchanged,
                format_float(session.total_price),
                remaining_time,
                if session.is_active { "🟢 Активна" } else { "🔴 Не активна" },
                format_user_bookings(&user_state.bookings, chat_id)
            ),
        )
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(make_session_management_keyboard())
        .await?;
    } else {
        bot.send_message(
            chat_id,
            "💰 *У вас пока нет активных сессий*\n\n\
            Чтобы начать, выберите консультанта и оплатите время сессии\\.",
        )
        .parse_mode(ParseMode::MarkdownV2)
        .await?;
    }
    
    Ok(())
}

/// Форматирование списка бронирований пользователя
pub fn format_user_bookings(bookings: &HashMap<String, Booking>, user_id: ChatId) -> String {
    let user_bookings: Vec<&Booking> = bookings.values()
        .filter(|b| b.user_id == user_id && !b.is_completed)
        .collect();
        
    if user_bookings.is_empty() {
        return "Нет активных бронирований".to_string();
    }
    
    user_bookings.iter()
        .enumerate()
        .map(|(i, booking)| {
            let assistant = AIAssistant::find_by_model(&booking.psychologist_model)
                .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
            format!(
                "{}\\. {} \\- {} мин \\({:.8} BTC\\) \\- {}",
                i + 1,
                assistant.name,
                booking.duration_minutes,
                format_float(booking.total_price),
                if booking.is_paid { "✅ Оплачено" } else { "⏳ Ожидает оплаты" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Отправка сообщения от AI-персоны
pub async fn send_ai_message(
    bot: &Bot,
    chat_id: ChatId,
    ai_name: &str,
    message: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let formatted_message = format!("*{}:* {}", escape_markdown_v2(ai_name), escape_markdown_v2(message));
    bot.send_message(chat_id, formatted_message)
        .parse_mode(ParseMode::MarkdownV2)
        .await?;
    Ok(())
}

pub async fn check_sessions_task(state: BotState) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    
    loop {
        interval.tick().await;
        
        let now = Utc::now();
        let user_states = state.get_all_user_states().await;
        
        for (chat_id, user_state) in user_states {
            if let Some(session) = &user_state.current_session {
                if session.is_active && now > session.paid_until {
                    let mut updated_state = user_state.clone();
                    if let Some(sess) = &mut updated_state.current_session {
                        sess.is_active = false;
                    }
                    
                    if let Err(e) = state.save_user_state(chat_id, updated_state).await {
                        log::error!("Error saving session state: {}", e);
                    }
                    
                    log::info!("Session expired for user {}", chat_id);
                }
            }
        }
    }
}
