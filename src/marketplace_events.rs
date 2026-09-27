use ring::hmac;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Deserialize)]
pub struct MarketplaceEvent {
    pub event_id: String,
    pub event_type: String,
    pub seller_id: String,
    pub buyer_id: Option<String>,
    pub order_id: Option<String>,
    pub asset_id: Option<String>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Handoff {
    StageSellerAsset { seller_id: String, asset_id: String },
    RefreshBuyer { buyer_id: String },
    ReleaseOrder { order_id: String, seller_id: String },
    Ignore { event_id: String },
}

#[derive(Debug, Error)]
pub enum EventError {
    #[error("signature must be hexadecimal")]
    SignatureEncoding,
    #[error("signature does not match")]
    InvalidSignature,
    #[error("{field} is required for {event_type}")]
    MissingField {
        field: &'static str,
        event_type: String,
    },
}

pub fn verify_signature(secret: &str, payload: &[u8], signature: &str) -> Result<(), EventError> {
    let supplied = decode_hex(signature)?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    hmac::verify(&key, payload, &supplied).map_err(|_| EventError::InvalidSignature)
}

fn decode_hex(value: &str) -> Result<Vec<u8>, EventError> {
    if value.len() % 2 != 0 {
        return Err(EventError::SignatureEncoding);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).map_err(|_| EventError::SignatureEncoding)?;
            u8::from_str_radix(pair, 16).map_err(|_| EventError::SignatureEncoding)
        })
        .collect()
}

pub fn decide(event: MarketplaceEvent) -> Result<Handoff, EventError> {
    match event.event_type.as_str() {
        "seller.asset.updated" => Ok(Handoff::StageSellerAsset {
            seller_id: event.seller_id,
            asset_id: required(event.asset_id, "asset_id", &event.event_type)?,
        }),
        "buyer.profile.updated" => Ok(Handoff::RefreshBuyer {
            buyer_id: required(event.buyer_id, "buyer_id", &event.event_type)?,
        }),
        "order.paid" => Ok(Handoff::ReleaseOrder {
            order_id: required(event.order_id, "order_id", &event.event_type)?,
            seller_id: event.seller_id,
        }),
        _ => Ok(Handoff::Ignore {
            event_id: event.event_id,
        }),
    }
}

fn required(
    value: Option<String>,
    field: &'static str,
    event_type: &str,
) -> Result<String, EventError> {
    value.ok_or_else(|| EventError::MissingField {
        field,
        event_type: event_type.to_owned(),
    })
}
