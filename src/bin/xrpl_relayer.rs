use dotenv::dotenv;
use std::sync::Arc;

use axelar_xrpl_relayer::{
    config::Config, distributor::Distributor, gmp_api, ingestor::Ingestor, queue::Queue,
    subscriber::Subscriber, xrpl_includer::XRPLIncluder,
};
use tokio::sync::watch;
use tracing::{self, Level};
use tracing_subscriber::FmtSubscriber;
use xrpl_types::AccountId;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = Config::from_env().map_err(|e| anyhow::anyhow!(e)).unwrap();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let tasks_queue = Arc::new(Queue::new(&config.queue_address, "tasks").await);
    let events_queue = Arc::new(Queue::new(&config.queue_address, "events").await);
    let gmp_api = Arc::new(gmp_api::GmpApi::new(&config.gmp_api_url, "xrpl").unwrap());
    let xrpl_includer =
        XRPLIncluder::new(config.includer_secret, config.refund_manager_address).await;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let account = AccountId::from_address(&config.multisig_address).unwrap();

    let mut subscriber = Subscriber::new_xrpl(&config.xrpl_rpc).await;
    let events_queue_ref = events_queue.clone();
    let subscriber_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            subscriber
                .run(account.to_address(), events_queue_ref, shutdown_rx)
                .await;
        }
    });

    let tasks_queue_ref = tasks_queue.clone();
    let includer_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            xrpl_includer.run(tasks_queue_ref, shutdown_rx).await;
        }
    });

    let events_queue_ref = events_queue.clone();
    let tasks_queue_ref = tasks_queue.clone();
    let ingestor = Ingestor::new(gmp_api.clone(), config.multisig_address.clone());
    let ingestor_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            ingestor
                .run(events_queue_ref, tasks_queue_ref, shutdown_rx)
                .await;
        }
    });

    let tasks_queue_ref = tasks_queue.clone();
    let gmp_api_ref = gmp_api.clone();
    let distributor_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            Distributor::run(gmp_api_ref, tasks_queue_ref, shutdown_rx).await;
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Shutting down...");
            let _ = shutdown_tx.send(true);
        }
    }

    let _ = tokio::join!(
        subscriber_handle,
        includer_handle,
        ingestor_handle,
        distributor_handle
    );
}
