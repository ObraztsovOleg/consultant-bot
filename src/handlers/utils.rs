use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup, ParseMode, ReplyMarkup};
use std::collections::HashMap;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc, TimeZone};

use crate::bot_state::BotState;
use crate::models::{AIAssistant, Booking, UserState};

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
    format!("{}", escape_markdown_v2(&assistant.name))
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

pub fn make_calendar_keyboard(selected_date: Option<DateTime<Utc>>) -> InlineKeyboardMarkup {
    let now = selected_date.unwrap_or(Utc::now());
    make_days_keyboard(now.year(), now.month())
}

pub fn make_days_keyboard(year: i32, month: u32) -> InlineKeyboardMarkup {
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    
    let month_names = [
        "Январь", "Февраль", "Март", "Апрель", "Май", "Июнь",
        "Июль", "Август", "Сентябрь", "Октябрь", "Ноябрь", "Декабрь"
    ];
    
    keyboard.push(vec![
        InlineKeyboardButton::callback("◀️", format!("calendar_prev_{}_{}", year, month)),
        InlineKeyboardButton::callback(
            format!("{} {}", month_names[month as usize - 1], year),
            "calendar_ignore".to_string()
        ),
        InlineKeyboardButton::callback("▶️", format!("calendar_next_{}_{}", year, month)),
    ]);
    
    // Дни недели
    keyboard.push(vec![
        InlineKeyboardButton::callback("Пн", "calendar_ignore".to_string()),
        InlineKeyboardButton::callback("Вт", "calendar_ignore".to_string()),
        InlineKeyboardButton::callback("Ср", "calendar_ignore".to_string()),
        InlineKeyboardButton::callback("Чт", "calendar_ignore".to_string()),
        InlineKeyboardButton::callback("Пт", "calendar_ignore".to_string()),
        InlineKeyboardButton::callback("Сб", "calendar_ignore".to_string()),
        InlineKeyboardButton::callback("Вс", "calendar_ignore".to_string()),
    ]);
    
    // Дни месяца
    let first_day = Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0).single().unwrap();
    let days_in_month = first_day.with_month(month + 1).unwrap_or(first_day.with_year(year + 1).unwrap().with_month(1).unwrap())
        .with_day(1).unwrap()
        .checked_sub_signed(Duration::days(1)).unwrap()
        .day();
    
    let mut current_week = Vec::new();
    let current_weekday = first_day.weekday().num_days_from_monday() as usize;
    let now = Utc::now();
    
    // Пустые ячейки перед первым днем
    for _ in 0..current_weekday {
        current_week.push(InlineKeyboardButton::callback(" ", "calendar_ignore".to_string()));
    }
    
    // Дни месяца
    for day in 1..=days_in_month {
        let day_date = Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).single().unwrap();
        
        // Блокируем прошедшие дни
        if day_date.date_naive() < now.date_naive() {
            current_week.push(InlineKeyboardButton::callback("❌", "calendar_ignore".to_string()));
        } else {
            let callback_data = format!("calendar_day_{}_{}_{}", year, month, day);
            current_week.push(InlineKeyboardButton::callback(day.to_string(), callback_data));
        }
        
        if current_week.len() == 7 {
            keyboard.push(current_week);
            current_week = Vec::new();
        }
    }
    
    // Пустые ячейки после последнего дня
    if !current_week.is_empty() {
        while current_week.len() < 7 {
            current_week.push(InlineKeyboardButton::callback(" ", "calendar_ignore".to_string()));
        }
        keyboard.push(current_week);
    }
    
    // Кнопка отмены
    keyboard.push(vec![InlineKeyboardButton::callback("❌ Отмена", "cancel_selection")]);
    
    InlineKeyboardMarkup::new(keyboard)
}

pub async fn make_time_keyboard(selected_date: DateTime<Utc>, state: Option<&BotState>) -> InlineKeyboardMarkup {
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    let now = Utc::now();
    
    // Получаем забронированные слоты для этой даты (включая неоплаченные в течение 5 минут)
    let booked_slots = if let Some(state) = state {
        state.get_booked_time_slots(selected_date).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    
    // Проверяем, что выбранная дата не в прошлом
    if selected_date.date_naive() < now.date_naive() {
        return InlineKeyboardMarkup::new(vec![
            vec![InlineKeyboardButton::callback("❌ Нельзя выбрать прошедшую дату", "calendar_ignore")],
            vec![InlineKeyboardButton::callback("◀️ Назад к календарю", format!("calendar_month_{}_{}", selected_date.year(), selected_date.month()))],
        ]);
    }
    
    // Генерируем временные слоты с 9:00 до 21:00 с интервалом 30 минут
    for hour in 9..=20 {
        for minute in &[0, 30] {
            let time_slot = selected_date.with_hour(hour).unwrap().with_minute(*minute).unwrap().with_second(0).unwrap();
            
            // Пропускаем прошедшее время (для сегодняшнего дня)
            if selected_date.date_naive() == now.date_naive() && time_slot <= now {
                // Блокируем кнопку для прошедшего времени
                let time_str = time_slot.format("%H:%M").to_string();
                keyboard.push(vec![InlineKeyboardButton::callback(
                    format!("❌ {} (прошло)", time_str), 
                    "calendar_ignore".to_string()
                )]);
                continue;
            }
            
            // Проверяем, забронирован ли этот слот (оплаченные ИЛИ неоплаченные в течение 5 минут)
            let is_booked = booked_slots.iter().any(|&booked_time| {
                booked_time.with_second(0).unwrap() == time_slot
            });
            
            let time_str = time_slot.format("%H:%M").to_string();
            
            if is_booked {
                // Блокируем забронированные слоты
                keyboard.push(vec![InlineKeyboardButton::callback(
                    format!("❌ {} (занято)", time_str), 
                    "calendar_ignore".to_string()
                )]);
            } else {
                let callback_data = format!("time_{}", time_slot.timestamp());
                keyboard.push(vec![InlineKeyboardButton::callback(time_str, callback_data)]);
            }
        }
    }
    
    // Если нет доступных слотов
    if keyboard.is_empty() || keyboard.iter().all(|row| row[0].text.contains("❌")) {
        keyboard.push(vec![InlineKeyboardButton::callback("❌ Нет доступных слотов на эту дату", "calendar_ignore")]);
    }
    
    // Кнопки навигации
    keyboard.push(vec![
        InlineKeyboardButton::callback("◀️ Назад к календарю", format!("calendar_month_{}_{}", selected_date.year(), selected_date.month())),
        InlineKeyboardButton::callback("❌ Отмена", "cancel_selection"),
    ]);
    
    InlineKeyboardMarkup::new(keyboard)
}

/// Настройки сессии
pub fn make_settings_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("📈 Низкая (0.1)", "temp_0.1"),
            InlineKeyboardButton::callback("🌡️ Средняя (0.3)", "temp_0.3"),
            InlineKeyboardButton::callback("🔥 Высокая (0.7)", "temp_0.7"),
        ],
        vec![InlineKeyboardButton::callback("👥 Сменить консультанта", "change_psychologist")],
        vec![InlineKeyboardButton::callback("🗑️ Очистить историю", "clear_history")],
    ])
}

pub fn make_session_management_keyboard(user_state: &UserState) -> InlineKeyboardMarkup {
    let mut keyboard = Vec::new();
    
    // Показываем кнопку "Отменить" для всех броней
    if let Some(session) = &user_state.current_session {
        if let Some(scheduled_start) = session.scheduled_start {
            if Utc::now() < scheduled_start && !session.is_active {
                keyboard.push(vec![
                    InlineKeyboardButton::callback("❌ Отменить сессию", "cancel_session"),
                ]);
            }
        }
    }
    
    keyboard.push(vec![InlineKeyboardButton::callback("📋 Новое бронирование", "new_booking")]);
    
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
        "💰 *Ваши сессии и бронирования*\n\nУ вас пока нет сессий или бронирований\\.".to_string()
    } else {
        "💰 *Ваши сессии и бронирования*\n\nВыберите сессию для просмотра информации или отмены:".to_string()
    };

    // Создаем клавиатуру с кнопками в две колонки
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    for booking in &user_bookings {
        let assistant = AIAssistant::find_by_model(&booking.psychologist_model)
            .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
        
        // Показываем кнопку отмены только для НЕзавершенных и НЕактивных сессий в будущем
        let can_cancel = !booking.is_completed && 
                        !booking.is_paid && 
                        booking.expires_at.map_or(false, |exp| exp > Utc::now()) ||
                        (booking.is_paid && 
                         !booking.is_completed && 
                         booking.scheduled_start.map_or(false, |start| start > Utc::now()));

        // Информационная кнопка
        let info_text = if let Some(scheduled_start) = booking.scheduled_start {
            format!("ℹ️ {} {}", assistant.name, scheduled_start.format("%d.%m %H:%M"))
        } else {
            format!("ℹ️ {} Немедленная", assistant.name)
        };

        if can_cancel {
            // Две кнопки в одной строке: информация слева, отмена справа
            keyboard.push(vec![
                InlineKeyboardButton::callback(info_text, format!("info_booking_{}", booking.id)),
                InlineKeyboardButton::callback("❌ Отменить", format!("cancel_booking_{}", booking.id))
            ]);
        } else {
            // Только информационная кнопка
            keyboard.push(vec![
                InlineKeyboardButton::callback(info_text, format!("info_booking_{}", booking.id))
            ]);
        }
    }

    // Добавляем кнопку нового бронирования
    if !user_bookings.is_empty() {
        keyboard.push(vec![
            InlineKeyboardButton::callback("📋 Новое бронирование", "new_booking")
        ]);
    }

    let reply_markup = InlineKeyboardMarkup::new(keyboard);

    bot.send_message(chat_id, sessions_text)
        .parse_mode(ParseMode::MarkdownV2)
        .reply_markup(reply_markup)
        .await?;

    Ok(())
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

pub fn has_active_session(user_state: &UserState) -> bool {
    if let Some(session) = &user_state.current_session {
        return session.is_active && Utc::now() < session.paid_until;
    }
    false
}