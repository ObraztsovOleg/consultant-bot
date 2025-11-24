pub mod commands;
pub mod messages;
pub mod callbacks;
pub mod payments;
pub mod utils;

pub use commands::command_handler;
pub use messages::message_handler;
pub use callbacks::callback_handler;
pub use payments::{pre_checkout_handler, successful_payment_handler};

use chrono::{Utc, Duration};
use tokio::time;
use crate::bot_state::BotState;
use teloxide::prelude::*;
use teloxide::{Bot, prelude::Requester};

pub async fn check_sessions_task(state: BotState) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    
    loop {
        interval.tick().await;
        
        let now = Utc::now();
        let user_states = state.get_all_user_states().await;
        
        // Очищаем просроченные брони
        if let Ok(deleted_count) = state.cleanup_expired_bookings().await {
            if deleted_count > 0 {
                log::info!("🧹 Cleaned up {} expired bookings in background", deleted_count);
            }
        }
        
        for (chat_id, user_state) in user_states {
            if let Some(session) = &user_state.current_session {
                // Проверяем запланированные сессии
                if let Some(scheduled_start) = session.scheduled_start {
                    if !session.is_active && now >= scheduled_start && now < session.paid_until {
                        // Активируем запланированную сессию
                        let mut updated_state = user_state.clone();
                        if let Some(sess) = &mut updated_state.current_session {
                            sess.is_active = true;
                        }
                        
                        if let Err(e) = state.save_user_state(chat_id, updated_state.clone()).await {
                            log::error!("Error activating scheduled session: {}", e);
                        } else {
                            log::info!("Scheduled session activated for user {}", chat_id);
                            
                            // Уведомляем пользователя
                            let bot = Bot::from_env();
                            let assistant = crate::models::AIAssistant::find_by_model(&session.psychologist_model)
                                .unwrap_or_else(|| crate::models::AIAssistant::get_all_assistants()[0].clone());
                                
                            let duration_minutes = session.paid_until.signed_duration_since(session.session_start).num_minutes();
                                
                            let _ = bot.send_message(
                                chat_id,
                                format!(
                                    "🎯 *Сессия началась\\!*\n\n\
                                    *Консультант:* {}\n\
                                    *Доступное время:* {} мин\n\n\
                                    Теперь вы можете общаться с консультантом\\.",
                                    crate::handlers::utils::escape_markdown_v2(&assistant.name),
                                    duration_minutes
                                ),
                            )
                            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                            .await;
                        }
                    }
                }
                
                // Проверяем истечение времени сессии
                if session.is_active && now > session.paid_until {
                    let mut updated_state = user_state.clone();
                    if let Some(sess) = &mut updated_state.current_session {
                        sess.is_active = false;
                    }
                    
                    // ПОМЕЧАЕМ БРОНЬ КАК ЗАВЕРШЕННУЮ
                    match state.find_booking_for_session(&session).await {
                        Ok(Some(booking)) => {
                            if !booking.is_completed {
                                if let Err(e) = state.mark_booking_completed(&booking.id).await {
                                    log::error!("Error marking booking as completed: {}", e);
                                } else {
                                    log::info!("✅ Session expired, booking {} marked as completed", booking.id);
                                }
                            }
                        }
                        Ok(None) => {
                            log::warn!("No booking found for expired session");
                        }
                        Err(e) => {
                            log::error!("Error finding booking for session: {}", e);
                        }
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

/// Отправка напоминания о сессии
async fn send_reminder(state: &BotState, chat_id: ChatId, booking_id: &str, minutes_left: i64) {
    if let Ok(bookings) = state.get_user_bookings(chat_id).await {
        if let Some(booking) = bookings.iter().find(|b| b.id == booking_id) {
            if booking.is_paid && !booking.is_completed {
                if let Some(scheduled_start) = booking.scheduled_start {
                    let bot = Bot::from_env();
                    let assistant = crate::models::AIAssistant::find_by_model(&booking.psychologist_model)
                        .unwrap_or_else(|| crate::models::AIAssistant::get_all_assistants()[0].clone());
                    
                    let _ = bot.send_message(
                        chat_id,
                        format!(
                            "🔔 *Напоминание о сессии*\n\n\
                            *До начала сессии осталось {} минут*\n\
                            *Консультант:* {}\n\
                            *Время начала:* {}\n\
                            *Продолжительность:* {} мин\n\n\
                            Подготовьтесь к сессии\\!",
                            minutes_left,
                            crate::handlers::utils::escape_markdown_v2(&assistant.name),
                            scheduled_start.format("%d\\.%m\\.%Y в %H:%M"),
                            booking.duration_minutes
                        ),
                    )
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await;
                    
                    log::info!("Sent {} minute reminder for user {}", minutes_left, chat_id);
                }
            }
        }
    }
}