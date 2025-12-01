use teloxide::prelude::*;
use teloxide::types::ParseMode;
use std::error::Error;

use crate::bot_state::BotState;
use crate::llm;
use crate::llm::config::ChatMessage;
use crate::models::{AIAssistant, PaymentConfig};
use crate::handlers::utils::{
    escape_markdown_v2, main_menu_keyboard, 
    make_ai_keyboard, make_consultants_info_keyboard, 
    send_ai_message, show_user_sessions
};
use chrono::Utc;

pub async fn message_handler(
    bot: Bot,
    msg: Message,
    state: BotState,
    _payment_config: PaymentConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(text) = msg.text() {
        // Пропускаем команды - они уже обработаны в command_handler
        if text.starts_with('/') {
            return Ok(());
        }

        match text {
            "👥 Выбрать консультанта" => {
                let keyboard = make_ai_keyboard(&state).await;
                bot.send_message(
                    msg.chat.id,
                    "👥 *Выберите консультанта:*\n\nКаждый консультант имеет свой стиль общения и индивидуальную цену\\.",
                )
                .parse_mode(ParseMode::MarkdownV2)
                .reply_markup(keyboard)
                .await?;
            }
            "💰 Мои сессии" => {
                show_user_sessions(&bot, msg.chat.id, &state).await?;
            }
            "ℹ️ Список консультантов" => {
                let keyboard = make_consultants_info_keyboard(&state).await;
                bot.send_message(
                    msg.chat.id,
                    "👥 *Список консультантов*\n\n\
Выберите консультанта чтобы увидеть подробную информацию:\n\n\
Каждый консультант — это стиль общения ИИ с разным характером и ценой\\.\n\
Это не психологи и не специалисты\\.",
                )
                .parse_mode(ParseMode::MarkdownV2)
                .reply_markup(keyboard)
                .await?;
            }
            "ℹ️ О боте" => {
                bot.send_message(
                    msg.chat.id,
                    "🫂 *О боте*\n\n\
                    Это AI\\-бот для общения и эмоциональной поддержки\n\n\
                    *Возможности:*\n\
                    • Выбор из нескольких консультантов\n\
                    • Оплата сессий через Telegram Stars\n\
                    • Контроль времени сессии\n\
                    • Полная конфиденциальность\n\n\
                    Используйте меню для навигации\\.",
                )
                .parse_mode(ParseMode::MarkdownV2)
                .await?;
            }
            _ => {
                let user_state = state.get_user_state(msg.chat.id).await;
                let assistants = AIAssistant::get_all_assistants(&state).await;
                let current_assistant = AIAssistant::find_by_model_with_price(&state, &user_state.current_model).await
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
                // Проверяем активность сессии
                let can_chat = if let Some(session) = &user_state.current_session {
                    session.is_active && Utc::now() < session.paid_until
                } else {
                    false
                };

                if !can_chat {
                    // Предлагаем выбрать консультанта для начала сессии
                    bot.send_message(
                        msg.chat.id,
                        format!(
                            "💬 *Чтобы начать сессию, необходимо выбрать консультанта:*\n\n"
                        ),
                    )
                    .parse_mode(ParseMode::MarkdownV2)
                    .reply_markup(make_ai_keyboard(&state).await)
                    .await?;
                    return Ok(());
                }

                // Показываем индикатор набора текста
                let _ = bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing).await;

                // ОБНОВЛЯЕМ СЕССИЮ В user_state
                let mut user_state = state.get_user_state(msg.chat.id).await;
                if let Some(session) = &mut user_state.current_session {
                    if session.history.is_empty() {
                        let telegram_prompt = format!(
                            "{}\n\n\
                            - Используй *жирный текст* для выделения важных моментов (одну звездочку вместо двух)\n\
                            - Используй _курсив_ для акцентов\n\
                            - Используй эмодзи для выразительности\n\
                            - Если нужно показать код, используй обратные кавычки: `код`\n\
                            - Для списков используй маркеры • или цифры\n\
                            - Используй MarkdownV2 для форматирования текста\n\
                            Твои будут отображаться в Telegram, поэтому форматируй их соответствующим образом",
                            current_assistant.prompt
                        );
                        
                        session.history.push(ChatMessage {
                            role: "system".to_string(),
                            content: Some(telegram_prompt),
                            tool_calls: None,
                            tool_call_id: None,
                            name: None
                        });
                    }

                    // Добавляем сообщение пользователя
                    session.history.push(ChatMessage {
                        role: "user".to_string(),
                        content: Some(text.to_string()),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None
                    });

                    log::info!("📝 Message added to history. Total messages: {}", session.history.len());

                    // Копия истории для LLM
                    let messages = session.history.clone();

                    // Отправка в LLM
                    let response = llm::chat(
                        messages,
                        current_assistant.model.clone(),
                        0.1
                    ).await?;
                    
                    if let Some(ai_response) = response.content {
                        // ДОБАВЛЯЕМ ПРОВЕРКУ И КОРРЕКЦИЮ ФОРМАТИРОВАНИЯ
                        let cleaned_response = clean_telegram_markdown(&ai_response);
                        
                        session.history.push(ChatMessage {
                            role: "assistant".to_string(),
                            content: Some(cleaned_response.clone()),
                            tool_calls: None,
                            tool_call_id: None,
                            name: None
                        });

                        session.messages_exchanged += 1;

                        if Utc::now() > session.paid_until {
                            session.is_active = false;
                            bot.send_message(
                                msg.chat.id,
                                "⏰ *Время сессии истекло*\n\nЧтобы продолжить, оплатите новое время сессии\\.",
                            )
                            .parse_mode(ParseMode::MarkdownV2)
                            .await?;
                        }

                        send_ai_message(&bot, msg.chat.id, &current_assistant.name, &cleaned_response).await?;

                        log::info!("💬 Response sent. Messages exchanged: {}", session.messages_exchanged);
                    } else {
                        log::error!("❌ LLM вернул пустой ответ");
                        bot.send_message(
                            msg.chat.id,
                            "Извините, произошла ошибка. Пожалуйста, попробуйте еще раз.",
                        )
                        .await?;
                    }

                    // Сохраняем user_state
                    if let Err(e) = state.save_user_state(msg.chat.id, user_state).await {
                        log::error!("❌ Error saving user state: {}", e);
                    } else {
                        log::info!("💾 User state saved successfully with updated history");
                    }
                } else {
                    log::error!("❌ No active session found for user {}", msg.chat.id);
                    bot.send_message(
                        msg.chat.id,
                        "⚠️ Сессия не найдена\\. Пожалуйста, начните новую сессию\\.",
                    )
                    .await?;
                }
            }
        }
    } else {
        bot.send_message(
            msg.chat.id,
            "👋 Напишите свой вопрос, консультант подключится и начнет с вами диалог.",
        )
        .reply_markup(main_menu_keyboard())
        .await?;
    }
    Ok(())
}

/// Функция для очистки и корректировки Markdown для Telegram
fn clean_telegram_markdown(text: &str) -> String {
    let mut cleaned = text.to_string();
    
    // Заменяем HTML-теги на Markdown
    cleaned = cleaned.replace("<b>", "*").replace("</b>", "*");
    cleaned = cleaned.replace("<strong>", "*").replace("</strong>", "*");
    cleaned = cleaned.replace("<i>", "_").replace("</i>", "_");
    cleaned = cleaned.replace("<em>", "_").replace("</em>", "_");
    cleaned = cleaned.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n");
    cleaned = cleaned.replace("<p>", "\n").replace("</p>", "\n");
    
    // Удаляем другие HTML-теги
    cleaned = cleaned.replace("<u>", "").replace("</u>", "");
    cleaned = cleaned.replace("<s>", "").replace("</s>", "");
    cleaned = cleaned.replace("<code>", "`").replace("</code>", "`");
    cleaned = cleaned.replace("<pre>", "```\n").replace("</pre>", "\n```");
    
    // Экранируем специальные символы MarkdownV2
    let specials = ['_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!'];
    let mut result = String::with_capacity(cleaned.len() * 2);
    
    for ch in cleaned.chars() {
        if specials.contains(&ch) {
            result.push('\\');
        }
        result.push(ch);
    }
    
    // Убираем лишние переносы строк
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }
    
    // Обрезаем слишком длинные сообщения (Telegram ограничение ~4096 символов)
    if result.len() > 3800 {
        result = result.chars().take(3800).collect();
        result.push_str("\n\n\\[Сообщение было сокращено\\]");
    }
    
    result
}