use marketplace_event_handoff::marketplace_events::{decide, Handoff, MarketplaceEvent};

#[test]
fn paid_order_releases_to_its_seller() {
    let event = MarketplaceEvent {
        event_id: "evt-104".to_owned(),
        event_type: "order.paid".to_owned(),
        seller_id: "seller-8".to_owned(),
        buyer_id: Some("buyer-3".to_owned()),
        order_id: Some("order-27".to_owned()),
        asset_id: None,
    };

    assert_eq!(
        decide(event).unwrap(),
        Handoff::ReleaseOrder {
            order_id: "order-27".to_owned(),
            seller_id: "seller-8".to_owned(),
        }
    );
}

