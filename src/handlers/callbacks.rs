use teloxide::prelude::*;
use teloxide::types::ParseMode;
use std::error::Error;
use uuid::Uuid;

use crate::bot_state::BotState;
use crate::models::{AIAssistant, PaymentConfig, Booking};
use crate::handlers::payments::send_ton_invoice;
use crate::handlers::utils::{
    escape_markdown_v2, format_float, make_ai_keyboard, make_booking_keyboard,
    make_settings_keyboard, make_session_management_keyboard, show_user_sessions,
    get_user_temperature
};
use chrono::Utc;

pub async fn callback_handler(
    bot: Bot,
    q: CallbackQuery,
    state: BotState,
    ton_config: PaymentConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(data) = q.data.as_deref() {
        if let Some(message) = q.message {
            let chat_id = message.chat().id;
            let message_id = message.id();

            match data {
                data if data.starts_with("select_ai_") => {
                    let model = data.strip_prefix("select_ai_").unwrap();
                    let assistant = AIAssistant::find_by_model(model)
                        .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
                    
                    let mut user_state = state.get_user_state(chat_id).await;
                    user_state.current_model = assistant.model.clone();
                    
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        format!(
                            "✅ *Вы выбрали:* {}\n\n*Стиль общения:* {}\n*Цена:* {} TON/мин\n\n{}\
                            \n\nВыберите продолжительность сессии:",
                            escape_markdown_v2(&assistant.name),
                            escape_markdown_v2(&assistant.specialty),
                            format_float(assistant.price_per_minute),
                            assistant.greeting
                        ),
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_booking_keyboard(&assistant))
                    .await?;
                    
                    if let Err(e) = state.save_user_state(chat_id, user_state).await {
                        log::error!("Error saving user state: {}", e);
                    }
                }

                data if data.starts_with("book_") => {
                    let parts: Vec<&str> = data.strip_prefix("book_").unwrap().split('_').collect();
                    if parts.len() == 2 {
                        let model = parts[0];
                        let duration: u32 = parts[1].parse().unwrap_or(30);
                        
                        let assistant = AIAssistant::find_by_model(model)
                            .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
                        
                        let (total_price, _) = assistant.calculate_price(duration);
                        let booking_id = Uuid::new_v4().to_string();
                        let invoice_payload = Uuid::new_v4().to_string();
                        
                        let booking = Booking {
                            id: booking_id.clone(),
                            user_id: chat_id,
                            psychologist_model: assistant.model.clone(),
                            duration_minutes: duration,
                            total_price,
                            ton_invoice_payload: invoice_payload.clone(),
                            is_paid: false,
                            is_completed: false,
                            created_at: Utc::now(),
                            payment_invoice_message_id: None,
                        };
                        
                        let mut user_state = state.get_user_state(chat_id).await;
                        user_state.bookings.insert(booking_id.clone(), booking.clone());
                        if let Err(e) = state.save_user_state(chat_id, user_state.clone()).await {
                            log::error!("Error saving user state: {}", e);
                        }

                        match send_ton_invoice(&bot, chat_id, &booking, &assistant, &ton_config).await {
                            Ok(invoice_message) => {
                                let mut user_state = state.get_user_state(chat_id).await;
                                if let Some(booking) = user_state.bookings.get_mut(&booking_id) {
                                    booking.payment_invoice_message_id = Some(invoice_message.id);
                                }
                                if let Err(e) = state.save_user_state(chat_id, user_state).await {
                                    log::error!("Error saving user state: {}", e);
                                }
                                
                                bot.delete_message(chat_id, message_id).await?;
                            }
                            Err(e) => {
                                log::error!("Failed to send invoice: {}", e);
                                bot.send_message(chat_id, "⚠️ Ошибка при создании счета. Попробуйте еще раз.")
                                    .await?;
                            }
                        }
                    }
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

                "extend_session" => {
                    let user_state = state.get_user_state(chat_id).await;
                    if let Some(session) = user_state.current_session {
                        let assistant = AIAssistant::find_by_model(&session.psychologist_model)
                            .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
                        
                        bot.send_message(
                            chat_id,
                            format!(
                                "⏱️ *Продление сессии*\n\n\
                                *Текущая консультант:* {}\n\
                                *Цена:* {} TON/мин\n\n\
                                Выберите продолжительность продления:",
                                escape_markdown_v2(&assistant.name),
                                format_float(assistant.price_per_minute)
                            ),
                        )
                        .parse_mode(ParseMode::MarkdownV2)
                        .reply_markup(make_booking_keyboard(&assistant))
                        .await?;
                    }
                }

                "end_session" => {
                    let mut user_state = state.get_user_state(chat_id).await;
                    if let Some(session) = &mut user_state.current_session {
                        session.is_active = false;
                        
                        bot.send_message(chat_id, "⏹️ *Сессия завершена*")
                            .parse_mode(ParseMode::MarkdownV2)
                            .await?;
                        if let Err(e) = state.save_user_state(chat_id, user_state).await {
                            log::error!("Error saving user state: {}", e);
                        }
                    }
                }

                "new_booking" => {
                    let user_state = state.get_user_state(chat_id).await;
                    let assistant = AIAssistant::find_by_model(&user_state.current_model)
                        .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
                    
                    bot.send_message(
                        chat_id,
                        format!(
                            "👥 *Новое бронирование*\n\n\
                            *Консультант:* {}\n\
                            *Цена:* {} TON/мин\n\n\
                            Выберите продолжительность сессии:",
                            escape_markdown_v2(&assistant.name),
                            format_float(assistant.price_per_minute)
                        ),
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_booking_keyboard(&assistant))
                    .await?;
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

                _ => {}
            }
        }
    }
    
    Ok(())
}
