use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use marketplace_event_handoff::infrai_client::InfraiClient;
use marketplace_event_handoff::marketplace_events::{decide, verify_signature, EventError, MarketplaceEvent};
use std::sync::Arc;

#[derive(Clone)]
struct ServiceState {
    secret: Arc<String>,
}

#[derive(Debug)]
enum ReceiverError {
    MissingSignature,
    Event(EventError),
    Json(serde_json::Error),
}

impl IntoResponse for ReceiverError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::MissingSignature => (StatusCode::UNAUTHORIZED, "missing signature".to_owned()),
            Self::Event(error @ EventError::InvalidSignature)
            | Self::Event(error @ EventError::SignatureEncoding) => {
                (StatusCode::UNAUTHORIZED, error.to_string())
            }
            Self::Event(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()),
            Self::Json(error) => (StatusCode::BAD_REQUEST, error.to_string()),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("setup") => setup().await?,
        Some("deliveries") => {
            let id = required_arg(args.next(), "webhook id")?;
            print_json(InfraiClient::from_env()?.deliveries(&id).await?)?;
        }
        Some("redrive") => {
            let queue = args.next().unwrap_or_else(|| "marketplace-events".to_owned());
            print_json(InfraiClient::from_env()?.redrive(&queue).await?)?;
        }
        Some("serve") => serve().await?,
        _ => eprintln!("usage: marketplace_handoff <setup|deliveries WEBHOOK_ID|redrive [QUEUE]|serve>"),
    }
    Ok(())
}

async fn setup() -> Result<(), Box<dyn std::error::Error>> {
    let client = InfraiClient::from_env()?;
    let secret = std::env::var("WEBHOOK_SECRET")?;
    let backend_url = std::env::var("MARKETPLACE_EVENT_URL")?;
    let queue = std::env::var("MARKETPLACE_QUEUE").unwrap_or_else(|_| "marketplace-events".to_owned());

    let webhook = client.register_marketplace_webhook(&queue, &secret).await?;
    let subscription = client.subscribe_backend(&queue, &backend_url).await?;
    print_json(serde_json::json!({"webhook": webhook, "queue_subscription": subscription}))?;
    Ok(())
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let state = ServiceState {
        secret: Arc::new(std::env::var("WEBHOOK_SECRET")?),
    };
    let app = Router::new()
        .route("/events/marketplace", post(receive_event))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("marketplace receiver listening on http://127.0.0.1:3000/events/marketplace");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn receive_event(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ReceiverError> {
    let signature = headers
        .get("X-Infrai-Signature")
        .and_then(|value| value.to_str().ok())
        .ok_or(ReceiverError::MissingSignature)?;
    verify_signature(&state.secret, &body, signature).map_err(ReceiverError::Event)?;
    let event: MarketplaceEvent = serde_json::from_slice(&body).map_err(ReceiverError::Json)?;
    let handoff = decide(event).map_err(ReceiverError::Event)?;
    Ok((StatusCode::ACCEPTED, Json(handoff)))
}

fn required_arg(value: Option<String>, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    value.ok_or_else(|| format!("missing {name}").into())
}

fn print_json(value: serde_json::Value) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

