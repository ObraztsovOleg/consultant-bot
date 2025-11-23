use teloxide::prelude::*;
use teloxide::types::{LabeledPrice, ParseMode};
use std::error::Error;

use crate::bot_state::BotState;
use crate::models::{PaymentConfig, Booking, AIAssistant, UserSession};
use crate::handlers::utils::{escape_markdown_v2, format_float};
use chrono::{Utc, Duration};

pub async fn send_ton_invoice(
    bot: &Bot,
    chat_id: ChatId,
    booking: &Booking,
    assistant: &AIAssistant,
    payment_config: &PaymentConfig,
) -> Result<Message, Box<dyn Error + Send + Sync>> {
    let (total_price_ton, _) = assistant.calculate_price(booking.duration_minutes);

    // Для тестового провайдера используем RUB и небольшую сумму
    let price_rub = 10000; // 100 рублей в копейках
    
    let prices = vec![LabeledPrice {
        label: format!("Сессия {} ({} мин)", assistant.name, booking.duration_minutes),
        amount: price_rub,
    }];

    let invoice = bot
        .send_invoice(
            chat_id,
            format!("Сессия с консультантом {}", assistant.name),
            format!(
                "Сессия\nДлительность: {} минут\n💎 Эквивалент: {} TON",
                booking.duration_minutes, total_price_ton
            ),
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
        
        log::info!("=== SUCCESSFUL PAYMENT ===");
        log::info!("Payment received for payload: {}", invoice_payload);
        
        let mut user_state = state.get_user_state(chat_id).await;
        
        // Находим бронирование
        let mut found_booking = None;
        for (booking_id, booking) in &user_state.bookings {
            if booking.ton_invoice_payload == *invoice_payload && !booking.is_paid {
                found_booking = Some((
                    booking_id.clone(),
                    booking.psychologist_model.clone(),
                    booking.duration_minutes,
                    booking.total_price,
                    booking.payment_invoice_message_id,
                ));
                break;
            }
        }
        
        if let Some((booking_id, ai_model, duration_minutes, total_price, invoice_msg_id)) = found_booking {
            log::info!("✅ Found booking to activate: {}", booking_id);
            
            // Обновляем бронирование
            if let Some(booking) = user_state.bookings.get_mut(&booking_id) {
                booking.is_paid = true;
                booking.is_completed = true;
            }
            
            // Создаем сессию
            let session = UserSession {
                chat_id,
                psychologist_model: ai_model.clone(),
                session_start: Utc::now(),
                paid_until: Utc::now() + Duration::minutes(duration_minutes as i64),
                total_price,
                messages_exchanged: 0,
                is_active: true,
                history: Vec::new(),
            };
            
            log::info!("🎯 Created session for AI persona: {}", ai_model);
            user_state.current_session = Some(session);
            
            // Удаляем сообщение с инвойсом если есть
            if let Some(invoice_msg_id) = invoice_msg_id {
                let _ = bot.delete_message(chat_id, invoice_msg_id).await;
                log::info!("Deleted invoice message");
            }
            
            // Сохраняем состояние
            if let Err(e) = state.save_user_state(chat_id, user_state).await {
                log::error!("❌ Error saving user state: {}", e);
            } else {
                log::info!("✅ User state saved successfully");
            }
            
            let assistant = AIAssistant::find_by_model(&ai_model)
                .unwrap_or_else(|| AIAssistant::get_all_assistants()[0].clone());
            
            bot.send_message(
                chat_id,
                format!(
                    "✅ *Оплата прошла успешно\\!*\n\n\
                    *Сессия началась*\n\
                    *Консультант:* {}\n\
                    *Доступное время:* {} мин\n\
                    *Стоимость:* {} TON\n\n\
                    Теперь вы можете общаться с консультантом\\.",
                    escape_markdown_v2(&assistant.name),
                    duration_minutes,
                    format_float(total_price)
                ),
            )
            .parse_mode(ParseMode::MarkdownV2)
            .await?;
            
        } else {
            log::warn!("❌ No booking found for payload: {}", invoice_payload);
            bot.send_message(chat_id, "⚠️ Бронирование не найдено. Свяжитесь с поддержкой.")
                .await?;
        }
        
    } else {
        log::warn!("No successful payment data in message");
    }
    
    Ok(())
}

pub async fn pre_checkout_handler(
    bot: Bot,
    q: PreCheckoutQuery,
    state: BotState,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let invoice_payload = &q.invoice_payload;
    
    log::info!("=== PRE-CHECKOUT ===");
    log::info!("Payload: {}", invoice_payload);
    
    let all_states = state.get_all_user_states().await;
    
    // Находим бронирование
    let mut found_booking = None;
    for (chat_id, user_state) in &all_states {
        for (booking_id, booking) in &user_state.bookings {
            if booking.ton_invoice_payload == *invoice_payload && !booking.is_paid {
                found_booking = Some((*chat_id, booking_id.clone(), booking.clone()));
                break;
            }
        }
    }
    
    if let Some((chat_id, booking_id, booking)) = found_booking {
        log::info!("✅ Confirming pre-checkout and ACTIVATING SESSION");
        
        let mut user_state = state.get_user_state(chat_id).await;
        
        if let Some(booking_entry) = user_state.bookings.get_mut(&booking_id) {
            booking_entry.is_paid = true;
            booking_entry.is_completed = true;
        }
        
        let session = UserSession {
            chat_id,
            psychologist_model: booking.psychologist_model.clone(),
            session_start: Utc::now(),
            paid_until: Utc::now() + Duration::minutes(booking.duration_minutes as i64),
            total_price: booking.total_price,
            messages_exchanged: 0,
            is_active: true,
            history: Vec::new(),
        };
        
        user_state.current_session = Some(session);
        
        if let Err(e) = state.save_user_state(chat_id, user_state).await {
            log::error!("Error saving session: {}", e);
        }
        
        bot.answer_pre_checkout_query(q.id, true).await?;
        
    } else {
        log::warn!("❌ Booking not found");
        bot.answer_pre_checkout_query(q.id, false)
            .error_message("Бронирование не найдено".to_string())
            .await?;
    }
    
    Ok(())
}

pub async fn shipping_query_handler(
    bot: Bot,
    q: ShippingQuery,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    bot.answer_shipping_query(q.id, false)
        .error_message("Доставка не требуется для цифровых услуг".to_string())
        .await?;
    Ok(())
}
