use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup, ParseMode, ReplyMarkup};
use chrono::Utc;

use crate::bot_state::BotState;
use crate::models::{AIAssistant, TimeSlot, UserState};

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

/// Главное меню
pub fn main_menu_keyboard() -> ReplyMarkup {
    ReplyMarkup::Keyboard(
        KeyboardMarkup::new(vec![
            vec![KeyboardButton::new("👥 Выбрать консультанта")],
            vec![KeyboardButton::new("💰 Мои сессии")],
            vec![KeyboardButton::new("ℹ️ Список консультантов"), KeyboardButton::new("ℹ️ О боте")],
        ])
        .resize_keyboard()
        .one_time_keyboard()
    )
}

/// Клавиатура выбора AI-персоны
pub async fn make_ai_keyboard(state: &BotState) -> InlineKeyboardMarkup {
    let assistants = AIAssistant::get_all_assistants(state).await;
    let mut keyboard = Vec::new();

    for assistant in assistants {
        keyboard.push(vec![InlineKeyboardButton::callback(
            format_ai_info(&assistant),
            format!("select_ai_{}", assistant.id), // Используем ID вместо model
        )]);
    }

    keyboard.push(vec![InlineKeyboardButton::callback("❌ Отмена", "cancel_selection")]);

    InlineKeyboardMarkup::new(keyboard)
}

/// Клавиатура с информацией о консультантах
pub async fn make_consultants_info_keyboard(state: &BotState) -> InlineKeyboardMarkup {
    let assistants = AIAssistant::get_all_assistants(state).await;
    let mut keyboard = Vec::new();

    for assistant in assistants {
        keyboard.push(vec![InlineKeyboardButton::callback(
            format!("ℹ️ {} - {}", assistant.name, assistant.specialty),
            format!("consultant_info_{}", assistant.id), // Используем ID вместо model
        )]);
    }

    keyboard.push(vec![InlineKeyboardButton::callback("👥 Выбрать консультанта", "change_consultant_from_list")]);

    InlineKeyboardMarkup::new(keyboard)
}

/// Клавиатура выбора времени сессии
pub async fn make_time_slots_keyboard(state: &BotState, assistant: &AIAssistant) -> InlineKeyboardMarkup {
    let time_slots = TimeSlot::get_all_active_slots(state).await;
    let mut keyboard = Vec::new();

    for slot in time_slots {
        let button_text = slot.format_price(assistant.price_per_minute);
        keyboard.push(vec![InlineKeyboardButton::callback(
            button_text,
            format!("time_slot_{}", slot.id),
        )]);
    }

    keyboard.push(vec![InlineKeyboardButton::callback("◀️ Назад к выбору консультанта", "back_to_consultant_selection")]);
    keyboard.push(vec![InlineKeyboardButton::callback("❌ Отмена", "cancel_selection")]);

    InlineKeyboardMarkup::new(keyboard)
}

/// Формат информации об AI-персоне
pub fn format_ai_info(assistant: &AIAssistant) -> String {
    format!("{} - {}", escape_markdown_v2(&assistant.name), escape_markdown_v2(&assistant.specialty))
}

/// Форматирование информации о консультанте для отображения
pub fn format_consultant_info(assistant: &AIAssistant) -> String {
    format!(
        "👤 *{}*\n\n\
        *Описание:* {}\n\
        *Специализация:* {}\n\
        *Цена:* {} Stars/мин",
        escape_markdown_v2(&assistant.name),
        escape_markdown_v2(&assistant.description),
        escape_markdown_v2(&assistant.specialty),
        (assistant.price_per_minute * 100.0) as i32,
    )
}

// Клавиатура для возврата к списку консультантов
pub fn make_back_to_consultants_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("◀️ Назад к списку консультантов", "back_to_consultants_list")],
        vec![InlineKeyboardButton::callback("👥 Выбрать консультанта", "change_consultant_from_list")],
    ])
}

pub fn make_session_management_keyboard(user_state: &UserState) -> InlineKeyboardMarkup {
    let mut keyboard = Vec::new();
    
    // Показываем кнопку "Отменить" для всех броней
    if let Some(session) = &user_state.current_session {
        if session.is_active && Utc::now() < session.paid_until {
            keyboard.push(vec![
                InlineKeyboardButton::callback("❌ Завершить сессию", "end_session"),
            ]);
        }
    }
    
    keyboard.push(vec![InlineKeyboardButton::callback("💬 Новая сессия", "new_session")]);
    
    InlineKeyboardMarkup::new(keyboard)
}

/// Получить температуру/креативность пользователя
pub async fn get_user_temperature(chat_id: ChatId, state: &BotState) -> f32 {
    let user_state = state.get_user_state(chat_id).await;
    user_state.user_temperatures.get(&chat_id).copied().unwrap_or(0.3)
}

pub async fn show_user_sessions(bot: &Bot, chat_id: ChatId, state: &BotState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Получаем все бронирования пользователя
    let user_bookings = match state.get_user_bookings(chat_id).await {
        Ok(bookings) => bookings,
        Err(_) => Vec::new(),
    };

    let sessions_text = if user_bookings.is_empty() {
        "💰 *Ваши сессии*\n\nУ вас пока нет активных сессий\\.".to_string()
    } else {
        "💰 *Ваши сессии*\n\nВыберите сессию для просмотра информации:".to_string()
    };

    // Создаем клавиатуру с кнопками
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    for booking in &user_bookings {
        let assistants = AIAssistant::get_all_assistants(state).await;
        let assistant = AIAssistant::find_by_model_with_price(&state, &booking.consultant_model).await
            .unwrap_or_else(|| {
                // Fallback если не найден в БД
                assistants.first()
                    .cloned()
                    .unwrap_or_else(|| AIAssistant {
                        id: 1,
                        name: "Анна".to_string(),
                        model: "GigaChat-2-Max".to_string(),
                        description: "Интерактивный помощник".to_string(),
                        specialty: "Общение и поддержка".to_string(),
                        greeting: "Здравствуйте!".to_string(),
                        prompt: "Ты помощник.".to_string(),
                        price_per_minute: 0.1,
                    })
            });
        
        // Информационная кнопка
        let info_text = format!("ℹ️ {} ({} мин)", assistant.name, booking.duration_minutes);

        keyboard.push(vec![
            InlineKeyboardButton::callback(info_text, format!("info_booking_{}", booking.id))
        ]);
    }

    // Добавляем кнопку новой сессии
    if !user_bookings.is_empty() {
        keyboard.push(vec![
            InlineKeyboardButton::callback("💬 Новая сессия", "new_session")
        ]);
    }

    let reply_markup = InlineKeyboardMarkup::new(keyboard);

    bot.send_message(chat_id, sessions_text)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(reply_markup)
        .await?;

    Ok(())
}

pub async fn send_ai_message(
    bot: &Bot,
    chat_id: ChatId,
    ai_name: &str,
    message: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let formatted_message = format!("*{}:* {}", escape_markdown_v2(ai_name), message);

    bot.send_message(chat_id, formatted_message)
        .parse_mode(ParseMode::MarkdownV2)
        .await?;
    Ok(())
}