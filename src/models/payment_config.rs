#[derive(Debug, Clone)]
pub struct PaymentConfig {
    pub provider_token: Option<String>,
    pub currency: String,
}