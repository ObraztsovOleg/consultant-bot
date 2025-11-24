use teloxide::prelude::*;
use teloxide::types::{LabeledPrice, ParseMode, MessageId, SuccessfulPayment};
use std::error::Error;
use chrono::{Utc, Duration};

use crate::bot_state::BotState;
use crate::models::{PaymentConfig, Booking, AIAssistant, UserSession};
use crate::handlers::utils::{escape_markdown_v2, format_float};

// ДИАГНОСТИЧЕСКАЯ ФУНКЦИЯ
pub async fn debug_payment_handler(
    bot: Bot,
    msg: Message,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    log::info!("🔍 DEBUG_PAYMENT_HANDLER called for chat: {}", msg.chat.id);
    
    if let Some(successful_payment) = msg.successful_payment() {
        log::info!("💰 SUCCESSFUL PAYMENT DETECTED IN DEBUG HANDLER!");
        log::info!("Invoice payload: {}", successful_payment.invoice_payload);
        log::info!("Total amount: {}", successful_payment.total_amount);
    } else {
        log::info!("❌ No successful payment in debug handler");
    }
    
    Ok(())
}

pub async fn send_ton_invoice(
    bot: &Bot,
    chat_id: ChatId,
    booking: &Booking,
    assistant: &AIAssistant,
    _payment_config: &PaymentConfig,
) -> Result<Message, Box<dyn Error + Send + Sync>> {
    let (total_price_ton, _) = assistant.calculate_price(booking.duration_minutes);

    // Для тестового провайдера используем RUB и небольшую сумму
    let price_rub = 10000; // 100 рублей в копейках
    
    let description = if let Some(scheduled_start) = booking.scheduled_start {
        format!(
            "Запланированная сессия\nКонсультант: {}\nДлительность: {} минут\nДата и время: {}\n💎 Эквивалент: {} TON",
            assistant.name,
            booking.duration_minutes,
            scheduled_start.format("%d.%m.%Y %H:%M"),
            total_price_ton
        )
    } else {
        format!(
            "Сессия\nКонсультант: {}\nДлительность: {} минут\n💎 Эквивалент: {} TON",
            assistant.name,
            booking.duration_minutes,
            total_price_ton
        )
    };

    let title = if booking.scheduled_start.is_some() {
        format!("Запланированная сессия с {}", assistant.name)
    } else {
        format!("Сессия с консультантом {}", assistant.name)
    };

    let prices = vec![LabeledPrice {
        label: format!("Сессия {} ({} мин)", assistant.name, booking.duration_minutes),
        amount: price_rub,
    }];

    log::info!("🔄 Sending invoice for booking {} to chat {}", booking.id, chat_id);
    log::info!("Invoice payload: {}", booking.ton_invoice_payload);

    let invoice = bot
        .send_invoice(
            chat_id,
            title,
            description,
            booking.ton_invoice_payload.clone(),
            "RUB",
            prices,
        )
        .provider_token("1744374395:TEST:43f2a7dbbf0320a34c41")
        .need_name(false)
        .need_phone_number(false)
        .need_email(false)
        .need_shipping_address(false)
        .is_flexible(false)
        .send()
        .await?;

    log::info!("✅ Invoice sent successfully for booking {}", booking.id);

    Ok(invoice)
}

pub async fn successful_payment_handler(
    bot: Bot,
    msg: Message,
    state: BotState,
    payment_config: PaymentConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    
    if let Some(successful_payment) = msg.successful_payment() {
        let chat_id = msg.chat.id;
        let invoice_payload = &successful_payment.invoice_payload;

        // Находим бронирование в отдельной таблице
        let booking = match state.get_booking_by_payload(invoice_payload).await {
            Ok(Some(booking)) => {
                log::info!("✅ Found booking: {}", booking.id);
                log::info!("Booking details: psychologist={}, scheduled_start={:?}, is_paid={}", 
                    booking.psychologist_model, booking.scheduled_start, booking.is_paid);
                booking
            },
            Ok(None) => {
                log::warn!("❌ No booking found for payload: {}", invoice_payload);
                
                // Попробуем найти по всем бронированиям пользователя
                if let Ok(user_bookings) = state.get_user_bookings(chat_id).await {
                    log::info!("User bookings count: {}", user_bookings.len());
                    for b in user_bookings {
                        log::info!("  Booking: id={}, payload={}, paid={}", b.id, b.ton_invoice_payload, b.is_paid);
                    }
                }
                
                bot.send_message(chat_id, "⚠️ Бронирование не найдено. Свяжитесь с поддержкой.")
                    .await?;
                return Ok(());
            }
            Err(e) => {
                log::error!("❌ Error finding booking: {}", e);
                bot.send_message(chat_id, "⚠️ Ошибка при поиске бронирования. Свяжитесь с поддержкой.")
                    .await?;
                return Ok(());
            }
        };
        
        if booking.is_paid {
            log::warn!("⚠️ Booking already paid: {}", booking.id);
            bot.send_message(chat_id, "ℹ️ Это бронирование уже было оплачено ранее.")
                .await?;
            return Ok(());
        }
        
        log::info!("🔄 Activating booking: {}", booking.id);
        
        // Обновляем бронирование
        let mut updated_booking = booking.clone();
        updated_booking.is_paid = true;
        updated_booking.is_completed = false;
        updated_booking.expires_at = None; // Убираем срок истечения для оплаченных броней
        
        if let Err(e) = state.save_booking(&updated_booking).await {
            log::error!("❌ Error updating booking: {}", e);
            bot.send_message(chat_id, "⚠️ Ошибка при обновлении статуса бронирования. Свяжитесь с поддержкой.")
                .await?;
            return Ok(());
        }
        
        log::info!("✅ Booking updated successfully: {}", updated_booking.id);
        
        let assistant = AIAssistant::find_by_model(&booking.psychologist_model)
            .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());

        let mut user_state = state.get_user_state(chat_id).await;
        
        if let Some(scheduled_start) = booking.scheduled_start {
            // ДЛЯ ЗАПЛАНИРОВАННОЙ СЕССИИ: создаем НЕактивную сессию
            let session = UserSession {
                chat_id,
                psychologist_model: booking.psychologist_model.clone(),
                session_start: scheduled_start,
                paid_until: scheduled_start + Duration::minutes(booking.duration_minutes as i64),
                total_price: booking.total_price,
                messages_exchanged: 0,
                is_active: false,
                history: Vec::new(),
                scheduled_start: Some(scheduled_start),
            };
            user_state.current_session = Some(session);
            
            let message_text = format!(
                "✅ *Сессия запланирована и оплачена\\!*\n\n\
                *Консультант:* {}\n\
                *Начало:* {}\n\
                *Продолжительность:* {} мин\n\
                *Стоимость:* {} TON\n\n\
                Сессия начнется автоматически в указанное время\\.",
                escape_markdown_v2(&assistant.name),
                escape_markdown_v2(&format!("{}", scheduled_start.format("%d.%m.%Y %H:%M"))),
                booking.duration_minutes,
                format_float(booking.total_price)
            );
            
            bot.send_message(chat_id, &message_text)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
            
            log::info!("📅 Scheduled session created for user {} at {}", chat_id, scheduled_start);
        } else {
            // ДЛЯ НЕМЕДЛЕННОЙ СЕССИИ: создаем активную сессию
            let session = UserSession {
                chat_id,
                psychologist_model: booking.psychologist_model.clone(),
                session_start: Utc::now(),
                paid_until: Utc::now() + Duration::minutes(booking.duration_minutes as i64),
                total_price: booking.total_price,
                messages_exchanged: 0,
                is_active: true,
                history: Vec::new(),
                scheduled_start: None,
            };
            user_state.current_session = Some(session);
            
            let message_text = format!(
                "✅ *Оплата прошла успешно\\!*\n\n\
                *Сессия началась*\n\
                *Консультант:* {}\n\
                *Доступное время:* {} мин\n\
                *Стоимость:* {} TON\n\n\
                Теперь вы можете общаться с консультантом\\.",
                escape_markdown_v2(&assistant.name),
                booking.duration_minutes,
                format_float(booking.total_price)
            );
            
            bot.send_message(chat_id, &message_text)
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
            
            log::info!("🎯 New active session created for user {}", chat_id);
        }
        
        // Удаляем сообщение с инвойсом если есть
        if let Some(invoice_msg_id) = booking.payment_invoice_message_id {
            match bot.delete_message(chat_id, invoice_msg_id).await {
                Ok(_) => log::info!("🗑️ Deleted invoice message"),
                Err(e) => log::warn!("⚠️ Could not delete invoice message: {}", e),
            }
        }
        
        // Сохраняем состояние пользователя
        if let Err(e) = state.save_user_state(chat_id, user_state).await {
            log::error!("❌ Error saving user state: {}", e);
            bot.send_message(chat_id, "⚠️ Ошибка при сохранении состояния. Сессия может работать некорректно.")
                .await?;
        } else {
            log::info!("💾 User state saved successfully for chat {}", chat_id);
        }
        
        log::info!("🎊 PAYMENT PROCESSING COMPLETED SUCCESSFULLY!");
        
    } else {
        bot.send_message( msg.chat.id, "⚠️ Не удалось обработать данные оплаты. Свяжитесь с поддержкой.")
            .await?;
    }
    
    Ok(())
}

pub async fn pre_checkout_handler(
    bot: Bot,
    q: PreCheckoutQuery,
    state: BotState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let invoice_payload = &q.invoice_payload;
    
    match state.get_booking_by_payload(invoice_payload).await {
        Ok(Some(booking)) => {
            if booking.is_paid {
                log::warn!("Booking already paid: {}", booking.id);
                bot.answer_pre_checkout_query(q.id, false)
                    .error_message("Бронирование уже оплачено".to_string())
                    .await?;
            } else {
                log::info!("✅ Confirming pre-checkout for booking: {}", booking.id);
                match bot.answer_pre_checkout_query(q.id, true).await {
                    Ok(_) => log::info!("✅ Pre-checkout confirmed"),
                    Err(e) => log::error!("❌ Error confirming pre-checkout: {}", e),
                }
            }
        }
        Ok(None) => {
            log::warn!("❌ Booking not found for payload: {}", invoice_payload);
            bot.answer_pre_checkout_query(q.id, false)
                .error_message("Бронирование не найдено".to_string())
                .await?;
        }
        Err(e) => {
            log::error!("❌ Error finding booking: {}", e);
            bot.answer_pre_checkout_query(q.id, false)
                .error_message("Ошибка при проверке бронирования".to_string())
                .await?;
        }
    }
    
    Ok(())
}
