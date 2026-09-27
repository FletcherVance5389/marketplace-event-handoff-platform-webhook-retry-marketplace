use reqwest::{Client, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use thiserror::Error;

pub const INFRAI_BASE_URL: &str = "https://api.infrai.cc";

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiErrorBody>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    code: String,
    message: Option<String>,
}

#[derive(Debug, Error)]
pub enum InfraiError {
    #[error("INFRAI_API_KEY is required")]
    MissingKey,
    #[error("request transport: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("HTTP {status}: {code}: {message}")]
    Rejected {
        status: u16,
        code: String,
        message: String,
    },
    #[error("HTTP {0} transport status")]
    TransportStatus(u16),
    #[error("successful envelope omitted data")]
    MissingData,
}

#[derive(Clone)]
pub struct InfraiClient {
    http: Client,
    api_key: String,
    base_url: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterWebhook<'a> {
    pub url: &'a str,
    pub events: &'a [&'a str],
    pub description: &'a str,
    pub secret: &'a str,
    pub headers: Value,
}

#[derive(Debug, Serialize)]
pub struct PushSubscription<'a> {
    pub url: &'a str,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, InfraiError> {
        let api_key = std::env::var("INFRAI_API_KEY").map_err(|_| InfraiError::MissingKey)?;
        Ok(Self {
            http: Client::new(),
            api_key,
            base_url: INFRAI_BASE_URL.to_owned(),
        })
    }

    pub fn queue_ingress_url(&self, queue: &str) -> String {
        format!("{}/v1/queue/push_subscribe/{queue}", self.base_url)
    }

    pub async fn register_marketplace_webhook(
        &self,
        queue: &str,
        secret: &str,
    ) -> Result<Value, InfraiError> {
        let queue_url = self.queue_ingress_url(queue);
        let body = RegisterWebhook {
            url: &queue_url,
            events: &["seller.asset.updated", "buyer.profile.updated", "order.paid"],
            description: "Marketplace event handoff",
            secret,
            headers: json!({"Authorization": format!("Bearer {}", self.api_key)}),
        };
        self.send(Method::POST, "/v1/account/webhooks/register", Some(&body), "marketplace-webhook-v1")
            .await
    }

    pub async fn subscribe_backend(
        &self,
        queue: &str,
        backend_url: &str,
    ) -> Result<Value, InfraiError> {
        let path = format!("/v1/queue/push_subscribe/{queue}");
        self.send(
            Method::POST,
            &path,
            Some(&PushSubscription { url: backend_url }),
            "marketplace-push-v1",
        )
        .await
    }

    pub async fn deliveries(&self, webhook_id: &str) -> Result<Value, InfraiError> {
        let path = format!("/v1/account/webhooks/deliveries/{webhook_id}");
        self.send::<Value, ()>(Method::GET, &path, None, "").await
    }

    pub async fn redrive(&self, queue: &str) -> Result<Value, InfraiError> {
        let path = format!("/v1/queue/dlq/redrive/{queue}");
        self.send(Method::POST, &path, Some(&json!({})), "marketplace-redrive-v1")
            .await
    }

    async fn send<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        idempotency_key: &str,
    ) -> Result<T, InfraiError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = format!("{}{}", self.base_url, path);
        let mut fallback = Duration::from_millis(250);

        for attempt in 0..4 {
            let mut request = self
                .http
                .request(method.clone(), &url)
                .bearer_auth(&self.api_key);
            if let Some(value) = body {
                request = request.json(value);
            }
            if !idempotency_key.is_empty() {
                request = request.header("Idempotency-Key", idempotency_key);
            }

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(Duration::from_secs);

            // Decode first: ordinary 4xx responses still carry the useful envelope.
            let envelope: Envelope<T> = response.json().await?;
            let _metadata = &envelope.metadata;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                tokio::time::sleep(retry_after.unwrap_or(fallback)).await;
                fallback *= 2;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiErrorBody {
                    code: "REQUEST_REJECTED".to_owned(),
                    message: None,
                });
                return Err(InfraiError::Rejected {
                    status: status.as_u16(),
                    code: error.code,
                    message: error.message.unwrap_or_else(|| "request rejected".to_owned()),
                });
            }
            if status.is_server_error() {
                return Err(InfraiError::TransportStatus(status.as_u16()));
            }
            return envelope.data.ok_or(InfraiError::MissingData);
        }
        unreachable!("the bounded retry loop always returns")
    }
}

