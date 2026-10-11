use shipment_onboarding_handoff::onboarding::{
    Channel, InfraiClient, ProofOfDelivery, Shipment, ShipmentEvent, ShipmentSignup,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let signup = ShipmentSignup {
        email: "chenhua@changba.com".to_owned(),
        password: "replace-with-a-secret-from-your-signup-form".to_owned(),
        name: "Morgan Lee".to_owned(),
        preferred_channel: Channel::Email,
        shipment: Shipment {
            tracking_number: "SHP-2048".to_owned(),
            latest_event: ShipmentEvent {
                status: "delivered".to_owned(),
                occurred_at: "2026-09-29T09:30:00Z".to_owned(),
                location: "Dock 4".to_owned(),
            },
            proof_of_delivery: Some(ProofOfDelivery {
                document_id: "pod-2048".to_owned(),
                filename: "signed-receipt.pdf".to_owned(),
            }),
            exception: None,
        },
    };

    let receipt = InfraiClient::from_env()?.onboard(&signup).await?;
    println!("account={} channel={:?} message_id={}", receipt.account, receipt.channel, receipt.message_id);
    Ok(())
}
