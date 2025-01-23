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
    utils::setup_logging,
    xrpl::{XrplIncluder, XrplTicketCreator},
};
use tokio::sync::watch;
use tracing_subscriber::{fmt, prelude::*, Registry};
use xrpl_types::AccountId;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = Config::from_env().map_err(|e| anyhow::anyhow!(e)).unwrap();

    setup_logging(&config);

    let events_queue = Queue::new(&config.queue_address, "events").await;
    let redis_client = redis::Client::open(config.redis_server.clone()).unwrap();
    let redis_pool = r2d2::Pool::builder().build(redis_client).unwrap();

    let account = AccountId::from_address(&config.xrpl_multisig).unwrap();

    let mut subscriber = Subscriber::new_xrpl(&config.xrpl_rpc, redis_pool.clone()).await;
    subscriber.run(account.to_address(), events_queue).await;
}
