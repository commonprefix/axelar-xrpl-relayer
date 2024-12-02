use axelar_xrpl_relayer::{
    queue::Queue,
    subscriber::{ChainTransaction, Subscriber},
};
use futures::StreamExt;
use tracing::{self, Level};
use tracing_subscriber::FmtSubscriber;
use xrpl_types::AccountId;

#[tokio::main]
async fn main() {
    // a builder for `FmtSubscriber`.
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(Level::TRACE)
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // Create RMQ connection
    let addr = "amqp://127.0.0.1:5672";
    let q = Queue::new(addr, "name_here").await;

    let account = AccountId::from_address("rP9iHnCmJcVPtzCwYJjU1fryC2pEcVqDHv").unwrap();
    let mut subscriber = Subscriber::new_xrpl("wss://s.devnet.rippletest.net:51233").await;
    let _ = subscriber.subscribe(account).await;
    while let Some(tx) = subscriber.transaction_stream().await.next().await {
        match tx {
            ChainTransaction::Xrpl(tx_event) => {
                let event_string = serde_json::to_string(&tx_event).unwrap();
                q.publish(event_string.as_bytes()).await;
            }
        }
    }

    // let consumer = q.consumer().await.unwrap();
    // while let Some(delivery) = consumer.next().await {
    //     let delivery = delivery.expect("error in consumer");
    //     info!("Received delivery: {:?}", delivery);
    //     let data = delivery.data.clone();
    //     info!(
    //         "Actual message: {:?}",
    //         std::str::from_utf8(&data).expect("utf8")
    //     );
    //     delivery.ack(BasicAckOptions::default()).await.expect("ack");
    // }
}
