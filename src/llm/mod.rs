pub mod config;

use std::env;
use std::fs;
use std::io;
use std::path::Path;

use reqwest::Client;
use reqwest_middleware::{ClientBuilder};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};

use anyhow::Result;

use crate::llm::config::ChatMessage;
use crate::llm::config::ServiceChatRequest;
use crate::llm::config::ServiceChatResponse;

const RETRIES: u32 = 1;
const LLM_SERVICE_HOST_ENV: &str = "LLM_SERVICE_HOST";

pub fn get_provider_from_model(model: &str) -> String {
    let model_lower = model.to_lowercase();
    if model_lower.contains("gigachat") {
        "gigachat".to_string()
    } else if model_lower.contains("deepseek") {
        "deepseek".to_string()
    } else {
        "unknown".to_string()
    }
}

pub async fn chat(
    messages: Vec<ChatMessage>,
    model: String,
    temperature: f32,
) -> Result<ServiceChatResponse> {
    let provider = get_provider_from_model(&model);
    let service_host = env::var(LLM_SERVICE_HOST_ENV)?;

    let request = ServiceChatRequest {
        provider,
        model,
        messages,
        temperature,
    };

    let retry_policy = ExponentialBackoff::builder()
        .build_with_max_retries(RETRIES);

    let client = ClientBuilder::new(Client::new())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();

    let response = client
        .post(format!("{}/chat", service_host))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(&request)?)
        .send()
        .await?;

    let text = response.text().await?;
    let response = serde_json::from_str::<ServiceChatResponse>(&text)?;

    Ok(response)
}