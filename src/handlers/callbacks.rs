use teloxide::prelude::*;
use teloxide::types::ParseMode;
use std::error::Error;
use uuid::Uuid;
use chrono::{DateTime, TimeZone, Utc, Datelike};

use crate::bot_state::BotState;
use crate::models::{AIAssistant, PaymentConfig, Booking};
use crate::handlers::payments::send_ton_invoice;
use crate::handlers::utils::{
    escape_markdown_v2, format_float, make_ai_keyboard, 
    make_calendar_keyboard, make_days_keyboard, make_time_keyboard, show_user_sessions
};

pub async fn callback_handler(
    bot: Bot,
    q: CallbackQuery,
    state: BotState,
    ton_config: PaymentConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(data) = q.data.as_deref() {
        if let Some(ref message) = q.message {
            let chat_id = message.chat().id;
            let message_id = message.id();

            match data {
                data if data.starts_with("select_ai_") => {
                    let model = data.strip_prefix("select_ai_").unwrap();
                    let assistant = AIAssistant::find_by_model_with_price(&state, model).await
                        .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
                    
                    let mut user_state = state.get_user_state(chat_id).await;
                    user_state.current_model = assistant.model.clone();
                    
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        format!(
                            "✅ *Вы выбрали:* {}\n\n*Стиль общения:* {}\n*Цена:* {} TON/мин\n\n{}\
                            \n\nВыберите дату и время для сессии:",
                            escape_markdown_v2(&assistant.name),
                            escape_markdown_v2(&assistant.specialty),
                            format_float(assistant.price_per_minute),
                            assistant.greeting
                        ),
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_calendar_keyboard(None))
                    .await?;
                    
                    if let Err(e) = state.save_user_state(chat_id, user_state).await {
                        log::error!("Error saving user state: {}", e);
                    }
                }

                "schedule_session" => {
                    let user_state = state.get_user_state(chat_id).await;
                    let assistant = AIAssistant::find_by_model_with_price(&state, &user_state.current_model).await
                        .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
                    
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        format!(
                            "📅 *Запланировать сессию*\n\n\
                            *Консультант:* {}\n\
                            *Цена:* {} TON/мин\n\n\
                            Выберите дату и время для сессии:",
                            escape_markdown_v2(&assistant.name),
                            format_float(assistant.price_per_minute)
                        ),
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_calendar_keyboard(None))
                    .await?;
                }

                data if data.starts_with("calendar_") => {
                    handle_calendar_callback(&bot, &q, &state, data).await?;
                }
                
                data if data.starts_with("time_") => {
                    handle_time_selection(&bot, &q, &state, data, &ton_config).await?;
                }

                "change_psychologist" => {
                    bot.edit_message_text(chat_id, message_id, "👥 *Выберите консультанта:*")
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(make_ai_keyboard())
                        .await?;
                }

                "clear_history" => {
                    let mut user_state = state.get_user_state(chat_id).await;
                    user_state.conversation_history.remove(&chat_id);
                    
                    bot.send_message(chat_id, "🗑️ История сессии очищена.")
                        .await?;
                    if let Err(e) = state.save_user_state(chat_id, user_state).await {
                        log::error!("Error saving user state: {}", e);
                    }
                }

                "new_booking" => {
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        "👥 *Выберите консультанта:*\n\nКаждый консультант имеет свой стиль общения и индивидуальную цену\\.",
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_ai_keyboard())
                    .await?;
                }
                
                // Обработчик информационной кнопки
                data if data.starts_with("info_booking_") => {
                    let booking_id = data.strip_prefix("info_booking_").unwrap();
                    
                    // Находим бронирование
                    match state.get_booking_by_id(booking_id).await {
                        Ok(Some(booking)) => {
                            if booking.user_id == chat_id {
                                let assistant = AIAssistant::find_by_model(&booking.psychologist_model)
                                    .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
                                
                                let schedule_info = if let Some(scheduled_start) = booking.scheduled_start {
                                    format!("📅 {}", scheduled_start.format("%d.%m.%Y %H:%M"))
                                } else {
                                    "⏱️ Немедленная".to_string()
                                };
                                
                                let status = if booking.is_paid {
                                    if booking.is_completed {
                                        "✅ Завершена"
                                    } else if let Some(scheduled_start) = booking.scheduled_start {
                                        if Utc::now() < scheduled_start {
                                            "⏳ Запланирована"
                                        } else {
                                            "🟢 Активна"
                                        }
                                    } else {
                                        "🟢 Активна"
                                    }
                                } else {
                                    if booking.expires_at.map_or(false, |exp| exp > Utc::now()) {
                                        "⏳ Ожидает оплаты"
                                    } else {
                                        "❌ Истекла"
                                    }
                                };

                                let info_text = format!(
                                    "📋 *Информация о сессии*\n\n\
                                    *Консультант:* {}\n\
                                    *Продолжительность:* {} мин\n\
                                    *Стоимость:* {} TON\n\
                                    *Время:* {}\n\
                                    *Статус:* {}\n\
                                    *ID брони:* `{}`",
                                    escape_markdown_v2(&assistant.name),
                                    booking.duration_minutes,
                                    format_float(booking.total_price),
                                    escape_markdown_v2(&schedule_info),
                                    escape_markdown_v2(status),
                                    booking.id
                                );

                                bot.send_message(chat_id, info_text)
                                    .parse_mode(ParseMode::MarkdownV2)
                                    .await?;
                            }
                        }
                        Ok(None) => {
                            bot.send_message(chat_id, "❌ Бронирование не найдено")
                                .await?;
                        }
                        Err(e) => {
                            log::error!("Error finding booking: {}", e);
                            bot.send_message(chat_id, "❌ Ошибка при поиске бронирования")
                                .await?;
                        }
                    }
                }

                // Обработчик отмены брони - ИСПРАВЛЕННАЯ ВЕРСИЯ
                data if data.starts_with("cancel_booking_") => {
                    let booking_id = data.strip_prefix("cancel_booking_").unwrap();
                    
                    // Находим бронирование
                    match state.get_booking_by_id(booking_id).await {
                        Ok(Some(booking)) => {
                            if booking.user_id == chat_id {
                                // Проверяем можно ли отменить
                                let can_cancel = !booking.is_completed && 
                                                !booking.is_paid && 
                                                booking.expires_at.map_or(false, |exp| exp > Utc::now()) ||
                                                (booking.is_paid && 
                                                !booking.is_completed && 
                                                booking.scheduled_start.map_or(false, |start| start > Utc::now()));

                                if can_cancel {
                                    // Удаляем ТОЛЬКО эту конкретную бронь
                                    if let Err(e) = sqlx::query("DELETE FROM bookings WHERE id = $1")
                                        .bind(booking_id)
                                        .execute(&state.db.pool)
                                        .await {
                                        log::error!("Error deleting booking: {}", e);
                                        bot.send_message(chat_id, "❌ Ошибка при отмене бронирования")
                                            .await?;
                                    } else {
                                        // Проверяем, была ли это текущая сессия - ТОЧНОЕ СРАВНЕНИЕ
                                        let mut user_state = state.get_user_state(chat_id).await;
                                        if let Some(session) = &user_state.current_session {
                                            if let (Some(session_start), Some(booking_start)) = (session.scheduled_start, booking.scheduled_start) {
                                                // Точное сравнение времени (до секунды)
                                                if session_start == booking_start {
                                                    user_state.current_session = None;
                                                    state.save_user_state(chat_id, user_state).await?;
                                                }
                                            }
                                        }

                                        // Обновляем сообщение со списком сессий
                                        show_user_sessions(&bot, chat_id, &state).await?;
                                        
                                        // Удаляем старое сообщение
                                        bot.delete_message(chat_id, message_id).await?;
                                        
                                        log::info!("✅ Booking {} cancelled by user {}", booking_id, chat_id);
                                    }
                                } else {
                                    bot.send_message(chat_id, "❌ Нельзя отменить эту сессию \\(уже началась или завершена\\)")
                                        .parse_mode(ParseMode::MarkdownV2)
                                        .await?;
                                }
                            } else {
                                bot.send_message(chat_id, "❌ Это не ваше бронирование")
                                    .await?;
                            }
                        }
                        Ok(None) => {
                            bot.send_message(chat_id, "❌ Бронирование не найдено")
                                .await?;
                        }
                        Err(e) => {
                            log::error!("Error finding booking: {}", e);
                            bot.send_message(chat_id, "❌ Ошибка при поиске бронирования")
                                .await?;
                        }
                    }
                }

                data if data.starts_with("temp_") => {
                    let temp_str = data.strip_prefix("temp_").unwrap();
                    if let Ok(temp) = temp_str.parse::<f32>() {
                        let mut user_state = state.get_user_state(chat_id).await;
                        user_state.user_temperatures.insert(chat_id, temp);
                        
                        let level = match temp {
                            x if x < 0.2 => "Низкая",
                            x if x < 0.5 => "Средняя",
                            _ => "Высокая",
                        };
                        
                        bot.send_message(
                            chat_id, 
                            format!("✅ Уровень эмпатии установлен: {} ({:.1})", level, temp)
                        ).await?;
                        if let Err(e) = state.save_user_state(chat_id, user_state).await {
                            log::error!("Error saving user state: {}", e);
                        }
                    }
                }

                "cancel_selection" => {
                    bot.edit_message_text(chat_id, message_id, "👥 Выбор консультанта отменен.")
                        .await?;
                }

                "calendar_ignore" => {
                    // Игнорируем нажатия на неактивные кнопки календаря
                }

                _ => {}
            }
        }
    }
    
    Ok(())
}

/// Обработчик календаря
async fn handle_calendar_callback(
    bot: &Bot,
    q: &CallbackQuery,
    state: &BotState,
    data: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(message) = &q.message {
        let chat_id = message.chat().id;
        let message_id = message.id();
        
        let parts: Vec<&str> = data.split('_').collect();
        if parts.len() >= 3 {
            let action = parts[1];
            let year = parts[2].parse::<i32>().unwrap_or(Utc::now().year());
            let month = if parts.len() > 3 { parts[3].parse::<u32>().unwrap_or(Utc::now().month()) } else { Utc::now().month() };
            let day = if parts.len() > 4 { parts[4].parse::<u32>().unwrap_or(1) } else { 1 };
            
            match action {
                "month" => {
                    // Показать дни месяца
                    let selected_date = Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0).single().unwrap();
                    bot.edit_message_reply_markup(chat_id, message_id)
                        .reply_markup(make_days_keyboard(year, month))
                        .await?;
                }
                "day" => {
                    // День выбран, показываем выбор времени
                    let selected_date = Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).single().unwrap();
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        format!(
                            "🕐 *Выберите время*\n\n\
                            *Дата:* {}\n\n\
                            Выберите удобное время для сессии:",
                            escape_markdown_v2(&format!("{}", selected_date.format("%d.%m.%Y")))
                        ),
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_time_keyboard(selected_date, Some(state)).await)
                    .await?;
                }
                "prev" | "next" => {
                    // Переключение месяцев
                    let new_month = if action == "prev" {
                        if month == 1 { 12 } else { month - 1 }
                    } else {
                        if month == 12 { 1 } else { month + 1 }
                    };
                    
                    let new_year = if action == "prev" && month == 1 {
                        year - 1
                    } else if action == "next" && month == 12 {
                        year + 1
                    } else {
                        year
                    };
                    
                    bot.edit_message_reply_markup(chat_id, message_id)
                        .reply_markup(make_days_keyboard(new_year, new_month))
                        .await?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

async fn handle_time_selection(
    bot: &Bot,
    q: &CallbackQuery,
    state: &BotState,
    data: &str,
    ton_config: &PaymentConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(message) = &q.message {
        let chat_id = message.chat().id;
        let message_id = message.id();
        
        let parts: Vec<&str> = data.split('_').collect();
        if parts.len() == 2 {
            let timestamp = parts[1].parse::<i64>().unwrap_or(0);
            let scheduled_time = DateTime::from_timestamp(timestamp, 0).unwrap_or(Utc::now());
            
            // ВАЛИДАЦИЯ: проверяем что выбранное время в будущем
            if scheduled_time <= Utc::now() {
                bot.send_message(
                    chat_id,
                    "❌ *Нельзя запланировать сессию на прошедшее время*\n\nПожалуйста, выберите другое время\\."
                )
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
                return Ok(());
            }
            
            let user_state = state.get_user_state(chat_id).await;
            let assistant = AIAssistant::find_by_model_with_price(&state, &user_state.current_model).await
                .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
            
            // Проверяем, не занят ли уже этот слот
            if let Ok(is_taken) = state.is_time_slot_taken(&assistant.model, scheduled_time).await {
                if is_taken {
                    bot.send_message(
                        chat_id,
                        "❌ *Это время уже занято*\n\nПожалуйста, выберите другое время\\."
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .await?;
                    return Ok(());
                }
            }
            
            // Фиксированная продолжительность 30 минут
            let duration_minutes = 30;
            let total_price = assistant.price_per_minute * duration_minutes as f64;
            let booking_id = Uuid::new_v4().to_string();
            let invoice_payload = Uuid::new_v4().to_string();
            
            let booking = Booking {
                id: booking_id.clone(),
                user_id: chat_id,
                psychologist_model: assistant.model.clone(),
                duration_minutes,
                total_price,
                ton_invoice_payload: invoice_payload.clone(),
                is_paid: false,
                is_completed: false,
                created_at: Utc::now(),
                payment_invoice_message_id: None,
                scheduled_start: Some(scheduled_time),
                expires_at: Some(Utc::now() + chrono::Duration::minutes(5)), // 5 минут на оплату
            };
            
            // Сохраняем бронирование в отдельную таблицу
            if let Err(e) = state.save_booking(&booking).await {
                log::error!("Error saving booking: {}", e);
                if e.to_string().contains("Time slot already taken") {
                    bot.send_message(
                        chat_id,
                        "❌ *Это время стало занято пока вы выбирали*\n\nПожалуйста, выберите другое время\\."
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .await?;
                } else {
                    bot.send_message(chat_id, "⚠️ Ошибка при создании бронирования. Попробуйте еще раз.")
                        .await?;
                }
                return Ok(());
            }

            match send_ton_invoice(&bot, chat_id, &booking, &assistant, ton_config).await {
                Ok(invoice_message) => {
                    // Обновляем booking с ID сообщения
                    let mut updated_booking = booking.clone();
                    updated_booking.payment_invoice_message_id = Some(invoice_message.id);
                    
                    if let Err(e) = state.save_booking(&updated_booking).await {
                        log::error!("Error updating booking with message ID: {}", e);
                    }
                    
                    bot.delete_message(chat_id, message_id).await?;
                    
                    // Отправляем сообщение о времени на оплату
                    bot.send_message(
                        chat_id,
                        "⏰ *У вас есть 5 минут чтобы оплатить сессию*\n\nПосле истечения этого времени бронь будет автоматически отменена\\."
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .await?;
                }
                Err(e) => {
                    log::error!("Failed to send invoice: {}", e);
                    bot.send_message(chat_id, "⚠️ Ошибка при создании счета. Попробуйте еще раз.")
                        .await?;
                }
            }
        }
    }
    Ok(())
}