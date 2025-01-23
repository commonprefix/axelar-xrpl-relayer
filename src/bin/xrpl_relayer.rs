use dotenv::dotenv;
use sentry_tracing::{layer as sentry_layer, EventFilter};
use std::sync::Arc;
use tracing::{error, warn, Level};

use axelar_xrpl_relayer::{
    config::Config,
    distributor::Distributor,
    gmp_api,
    ingestor::Ingestor,
    queue::Queue,
    subscriber::Subscriber,
    xrpl::{XrplIncluder, XrplTicketCreator},
};
use tokio::sync::watch;
use tracing_subscriber::{fmt, prelude::*, Registry};
use xrpl_types::AccountId;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = Config::from_env().map_err(|e| anyhow::anyhow!(e)).unwrap();

    let _guard = sentry::init((
        config.xrpl_relayer_sentry_dsn.to_string(),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            traces_sample_rate: 1.0,
            ..Default::default()
        },
    ));

    let fmt_layer = fmt::layer().with_target(true);

    let sentry_layer = sentry_layer().event_filter(|metadata| match *metadata.level() {
        Level::ERROR => EventFilter::Event, // Send `error` events to Sentry
        Level::WARN => EventFilter::Event,  // Send `warn` events to Sentry
        _ => EventFilter::Ignore,           // Ignore other levels
    });

    let subscriber = Registry::default()
        .with(fmt_layer) // Console logging
        .with(sentry_layer); // Sentry logging

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global tracing subscriber");

    let tasks_queue_arc = Queue::new(&config.queue_address, "tasks").await;
    let events_queue_arc = Queue::new(&config.queue_address, "events").await;
    let gmp_api = Arc::new(gmp_api::GmpApi::new(&config.gmp_api_url, "xrpl").unwrap());
    let redis_client = redis::Client::open(config.redis_server.clone()).unwrap();
    let redis_pool = r2d2::Pool::builder().build(redis_client).unwrap();
    let xrpl_includer = XrplIncluder::new(config.clone(), gmp_api.clone())
        .await
        .unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let account = AccountId::from_address(&config.xrpl_multisig).unwrap();

    let mut subscriber = Subscriber::new_xrpl(&config.xrpl_rpc, redis_pool.clone()).await;
    let events_queue_clone = events_queue_arc.clone();
    let subscriber_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            subscriber
                .run(account.to_address(), events_queue_clone, shutdown_rx)
                .await;
        }
    });

    let tasks_queue_clone = tasks_queue_arc.clone();
    let includer_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            xrpl_includer.run(tasks_queue_clone, shutdown_rx).await;
        }
    });

    let events_queue_clone = events_queue_arc.clone();
    let tasks_queue_clone = tasks_queue_arc.clone();
    let ingestor = Ingestor::new(gmp_api.clone(), config.clone());
    let ingestor_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            ingestor
                .run(events_queue_clone, tasks_queue_clone, shutdown_rx)
                .await;
        }
    });

    let gmp_api_ref = gmp_api.clone();
    let tasks_queue_clone = tasks_queue_arc.clone();
    let mut distributor = Distributor::new(redis_pool.clone());
    let distributor_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            distributor
                .run(gmp_api_ref, tasks_queue_clone, shutdown_rx)
                .await;
        }
    });

    let ticket_creator = XrplTicketCreator::new(gmp_api.clone(), config.clone());
    let ticket_creator_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            ticket_creator.run(shutdown_rx).await;
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
        distributor_handle,
        ticket_creator_handle
    );
}
