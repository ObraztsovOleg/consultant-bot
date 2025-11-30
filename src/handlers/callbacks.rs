use teloxide::prelude::*;
use teloxide::types::ParseMode;
use std::error::Error;
use uuid::Uuid;
use chrono::{Utc, Duration};

use crate::bot_state::BotState;
use crate::models::{AIAssistant, PaymentConfig, Booking, TimeSlot};
use crate::handlers::payments::send_stars_invoice;
use crate::handlers::utils::{
    escape_markdown_v2, make_ai_keyboard, 
    make_consultants_info_keyboard, format_consultant_info, make_back_to_consultants_keyboard,
    make_time_slots_keyboard
};

pub async fn callback_handler(
    bot: Bot,
    q: CallbackQuery,
    state: BotState,
    payment_config: PaymentConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(data) = q.data.as_deref() {
        if let Some(ref message) = q.message {
            let chat_id = message.chat().id;
            let message_id = message.id();

            match data {
                data if data.starts_with("select_ai_") => {
                    let model = data.strip_prefix("select_ai_").unwrap();
                    let assistants = AIAssistant::get_all_assistants(&state).await;
                    let assistant = AIAssistant::find_by_model_with_price(&state, model).await
                        .unwrap_or_else(|| {
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
                    
                    let mut user_state = state.get_user_state(chat_id).await;
                    user_state.current_model = assistant.model.clone();
                    
                    // Сохраняем выбор консультанта
                    if let Err(e) = state.save_user_state(chat_id, user_state).await {
                        log::error!("Error saving user state: {}", e);
                    }

                    // Показываем выбор времени сессии
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        format!(
                            "✅ *Вы выбрали:* {}\n\n*Стиль общения:* {}\n*Цена:* {} Stars/мин\n\n{}\
                            \n\nВыберите продолжительность сессии:",
                            escape_markdown_v2(&assistant.name),
                            escape_markdown_v2(&assistant.specialty),
                            (assistant.price_per_minute * 100.0) as i32,
                            escape_markdown_v2(&assistant.greeting)
                        ),
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_time_slots_keyboard(&state, &assistant).await)
                    .await?;
                }

                // Обработчик информации о консультанте
                data if data.starts_with("consultant_info_") => {
                    let model = data.strip_prefix("consultant_info_").unwrap();
                    let assistants = AIAssistant::get_all_assistants(&state).await;
                    let assistant = AIAssistant::find_by_model_with_price(&state, model).await
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
                    
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        format_consultant_info(&assistant),
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_back_to_consultants_keyboard())
                    .await?;
                }

                // Обработчик возврата к списку консультантов
                "back_to_consultants_list" => {
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        "👥 *Список консультантов*\n\n\
Выберите консультанта чтобы увидеть подробную информацию:\n\n\
Каждый консультант — это стиль общения ИИ с разным характером и ценой\\.\n\
Это не психологи и не специалисты\\.",
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_consultants_info_keyboard(&state).await)
                    .await?;
                }

                data if data.starts_with("time_slot_") => {
                    let slot_id = data.strip_prefix("time_slot_").unwrap().parse::<i32>().unwrap_or(0);
                    
                    let user_state = state.get_user_state(chat_id).await;
                    let assistants = AIAssistant::get_all_assistants(&state).await;
                    let assistant = AIAssistant::find_by_model_with_price(&state, &user_state.current_model).await
                        .unwrap_or_else(|| {
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
                
                    let time_slots = TimeSlot::get_all_active_slots(&state).await;
                    let selected_slot = time_slots.iter().find(|slot| slot.id == slot_id)
                        .unwrap_or(&time_slots[0]);
                
                    let duration_minutes = selected_slot.duration_minutes as u32;
                    let total_price = selected_slot.calculate_price(assistant.price_per_minute);
                    
                    let booking_id = Uuid::new_v4().to_string();
                    let invoice_payload = Uuid::new_v4().to_string();
                    
                    let booking = Booking {
                        id: booking_id.clone(),
                        user_id: chat_id,
                        consultant_model: assistant.model.clone(),
                        duration_minutes,
                        total_price,
                        invoice_payload: invoice_payload.clone(),
                        is_paid: false,
                        is_completed: false,
                        created_at: Utc::now(),
                        payment_invoice_message_id: None,
                        expires_at: Some(Utc::now() + Duration::minutes(5)), // 5 минут на оплату
                    };
                    
                    // Сохраняем бронирование
                    if let Err(e) = state.save_booking(&booking).await {
                        log::error!("Error saving booking: {}", e);
                        bot.send_message(chat_id, "⚠️ Ошибка при создании сессии. Попробуйте еще раз.")
                            .await?;
                        return Ok(());
                    }
                    
                    log::info!("Booking created: {:?}", booking);

                    match send_stars_invoice(&bot, chat_id, &booking, &assistant, &payment_config).await {
                        Ok(invoice_message) => {
                            let mut updated_booking = booking.clone();
                            updated_booking.payment_invoice_message_id = Some(invoice_message.id);
                            
                            if let Err(e) = state.save_booking(&updated_booking).await {
                                log::error!("Error updating booking with message ID: {}", e);
                            }
                            
                            bot.delete_message(chat_id, message_id).await?;
                            
                            bot.send_message(
                                chat_id,
                                "⏰ *У вас есть 5 минут чтобы оплатить сессию*\n\nПосле истечения этого времени сессия будет автоматически отменена\\."
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

                // Обработчик возврата к выбору консультанта
                "back_to_consultant_selection" => {
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        "👥 *Выберите консультанта:*\n\nКаждый консультант имеет свой стиль общения и индивидуальную цену\\.",
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_ai_keyboard(&state).await)
                    .await?;
                }

                // Обработчик перехода к выбору консультанта из списка
                "change_consultant_from_list" => {
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        "👥 *Выберите консультанта:*\n\nКаждый консультант имеет свой стиль общения и индивидуальную цену\\.",
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_ai_keyboard(&state).await)
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

                "new_session" => {
                    bot.edit_message_text(
                        chat_id,
                        message_id,
                        "👥 *Выберите консультанта:*\n\nКаждый консультант имеет свой стиль общения и индивидуальную цену\\.",
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_ai_keyboard(&state).await)
                    .await?;
                }
                
                // Обработчик информационной кнопки
                data if data.starts_with("info_booking_") => {
                    let booking_id = data.strip_prefix("info_booking_").unwrap();
                    
                    // Находим бронирование
                    match state.get_booking_by_id(booking_id).await {
                        Ok(Some(booking)) => {
                            if booking.user_id == chat_id {
                                let assistants = AIAssistant::get_all_assistants(&state).await;
                                let assistant = AIAssistant::find_by_model(&state, &booking.consultant_model).await
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
                                
                                let status = if booking.is_paid {
                                    if booking.is_completed {
                                        "✅ Завершена"
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
                                    *Стоимость:* {} Stars\n\
                                    *Статус:* {}\n\
                                    *ID сессии:* `{}`",
                                    escape_markdown_v2(&assistant.name),
                                    booking.duration_minutes,
                                    (booking.total_price * 100.0) as i32,
                                    escape_markdown_v2(status),
                                    booking.id
                                );

                                bot.send_message(chat_id, info_text)
                                    .parse_mode(ParseMode::MarkdownV2)
                                    .await?;
                            }
                        }
                        Ok(None) => {
                            bot.send_message(chat_id, "❌ Сессия не найдена")
                                .await?;
                        }
                        Err(e) => {
                            log::error!("Error finding booking: {}", e);
                            bot.send_message(chat_id, "❌ Ошибка при поиске сессии")
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
                    bot.edit_message_text(chat_id, message_id, "❌ Выбор отменен.")
                        .await?;
                }

                _ => {}
            }
        }
    }
    
    Ok(())
}
