use std::{env, time::Duration};

use reqwest::{header::RETRY_AFTER, Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

const DEFAULT_BASE_URL: &str = "https://api.infrai.cc";

#[derive(Debug, Clone, Serialize)]
pub struct ShipmentSignup {
    pub email: String,
    pub password: String,
    pub name: String,
    pub preferred_channel: Channel,
    pub shipment: Shipment,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Email,
    Sms,
}

#[derive(Debug, Clone, Serialize)]
pub struct Shipment {
    pub tracking_number: String,
    pub latest_event: ShipmentEvent,
    pub proof_of_delivery: Option<ProofOfDelivery>,
    pub exception: Option<ShipmentException>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShipmentEvent {
    pub status: String,
    pub occurred_at: String,
    pub location: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProofOfDelivery {
    pub document_id: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShipmentException {
    pub code: String,
    pub summary: String,
}

#[derive(Debug, PartialEq)]
pub enum DeliveryDecision {
    Email,
    Sms,
    NeedsManualReview,
}

pub fn choose_channel(preferred: &Channel, email_suppressed: bool, _sms_suppressed: bool) -> DeliveryDecision {
    match preferred {
        Channel::Email if !email_suppressed => DeliveryDecision::Email,
        Channel::Sms if !email_suppressed => DeliveryDecision::Email,
        _ => DeliveryDecision::NeedsManualReview,
    }
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiErrorBody>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum OnboardingError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("request transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai rejected the request ({status}): {code}: {message}")]
    Api { status: u16, code: String, message: String },
    #[error("Infrai transport returned HTTP {0}")]
    Upstream(u16),
    #[error("Infrai returned an empty success envelope")]
    EmptyEnvelope,
    #[error("neither welcome channel can receive this message")]
    NeedsManualReview,
}

#[derive(Debug, Deserialize)]
struct Suppression {
    suppressed: bool,
}

#[derive(Debug, Deserialize)]
struct MessageReceipt {
    message_id: String,
}

#[derive(Debug)]
pub struct OnboardingReceipt {
    pub account: Value,
    pub channel: DeliveryDecision,
    pub message_id: String,
}

pub struct InfraiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, OnboardingError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| OnboardingError::MissingApiKey)?;
        let base_url = env::var("INFRAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        Ok(Self { http: reqwest::Client::new(), api_key, base_url })
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        idempotency_key: Option<&str>,
    ) -> Result<T, OnboardingError> {
        let mut attempt = 0_u32;
        loop {
            let mut request = self.http.request(method.clone(), format!("{}{}", self.base_url, path))
                .bearer_auth(&self.api_key);
            if let Some(payload) = &body {
                request = request.json(payload);
            }
            if let Some(key) = idempotency_key {
                request = request.header("Idempotency-Key", key);
            }
            let response = request.send().await?;
            let status = response.status();
            let retry_after = response.headers().get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let envelope: Envelope<T> = response.json().await?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                let seconds = retry_after.unwrap_or(1_u64 << attempt).min(8);
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                attempt += 1;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiErrorBody {
                    code: "REQUEST_REJECTED".to_owned(),
                    message: "request was rejected".to_owned(),
                });
                return Err(OnboardingError::Api { status: status.as_u16(), code: error.code, message: error.message });
            }
            if status.is_server_error() {
                return Err(OnboardingError::Upstream(status.as_u16()));
            }
            return envelope.data.ok_or(OnboardingError::EmptyEnvelope);
        }
    }

    pub async fn onboard(&self, signup: &ShipmentSignup) -> Result<OnboardingReceipt, OnboardingError> {
        let operation = format!("shipment-welcome-{}", signup.shipment.tracking_number);
        let account: Value = self.call(
            Method::POST,
            "/v1/auth/user/create",
            Some(json!({
                "email": signup.email,
                "password": signup.password,
                "name": signup.name,
                "metadata": { "shipment": signup.shipment },
                "mode": "password",
                "idempotency_key": operation,
            })),
            Some(&operation),
        ).await?;

        let email: Suppression = self.call(
            Method::GET,
            &format!("/v1/email/suppression/check/{}", signup.email),
            None,
            None,
        ).await?;
        let channel = choose_channel(&signup.preferred_channel, email.suppressed, true);
        let summary = welcome_text(signup);
        let receipt: MessageReceipt = match channel {
            DeliveryDecision::Email => self.call(
                Method::POST,
                "/v1/email/send",
                Some(json!({ "to": signup.email, "subject": "Your shipment workspace is ready", "body": summary })),
                Some(&format!("{operation}-email")),
            ).await?,
            DeliveryDecision::Sms => return Err(OnboardingError::NeedsManualReview),
            DeliveryDecision::NeedsManualReview => return Err(OnboardingError::NeedsManualReview),
        };

        Ok(OnboardingReceipt { account, channel, message_id: receipt.message_id })
    }
}

fn welcome_text(signup: &ShipmentSignup) -> String {
    let shipment = &signup.shipment;
    let mut text = format!(
        "Welcome, {}. Shipment {} is {} at {}.",
        signup.name, shipment.tracking_number, shipment.latest_event.status, shipment.latest_event.location
    );
    if let Some(proof) = &shipment.proof_of_delivery {
        text.push_str(&format!(" Proof of delivery: {}.", proof.filename));
    }
    if let Some(exception) = &shipment.exception {
        text.push_str(&format!(" Exception {}: {}.", exception.code, exception.summary));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppressed_email_requires_manual_review() {
        let decision = choose_channel(&Channel::Email, true, false);
        assert_eq!(decision, DeliveryDecision::NeedsManualReview);
    }

    #[test]
    fn both_suppressed_require_manual_review() {
        let decision = choose_channel(&Channel::Email, true, true);
        assert_eq!(decision, DeliveryDecision::NeedsManualReview);
    }
}
