pub mod commands;
pub mod messages;
pub mod callbacks;
pub mod payments;
pub mod utils;

pub use commands::command_handler;
pub use messages::message_handler;
pub use callbacks::callback_handler;
pub use payments::{pre_checkout_handler, shipping_query_handler, successful_payment_handler};

use chrono::Utc;
use tokio::time;
use crate::bot_state::BotState;
pub async fn check_sessions_task(state: BotState) {
    let mut interval = time::interval(time::Duration::from_secs(60));
    
    loop {
        interval.tick().await;
        
        let now = Utc::now();
        let user_states = state.get_all_user_states().await;
        
        for (chat_id, user_state) in user_states {
            if let Some(session) = &user_state.current_session {
                if session.is_active && now > session.paid_until {
                    let mut updated_state = user_state.clone();
                    if let Some(sess) = &mut updated_state.current_session {
                        sess.is_active = false;
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