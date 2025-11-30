pub mod commands;
pub mod messages;
pub mod callbacks;
pub mod payments;
pub mod utils;

pub use commands::command_handler;
pub use messages::message_handler;
pub use callbacks::callback_handler;
pub use payments::{pre_checkout_handler, successful_payment_handler};

use chrono::Utc;
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