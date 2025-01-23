use dotenv::dotenv;
use std::sync::Arc;

use axelar_xrpl_relayer::{
    config::Config, gmp_api, ingestor::Ingestor, queue::Queue, utils::setup_logging,
};

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = Config::from_env().map_err(|e| anyhow::anyhow!(e)).unwrap();

    let _guard = setup_logging(&config);

    let tasks_queue = Queue::new(&config.queue_address, "tasks").await;
    let events_queue = Queue::new(&config.queue_address, "events").await;
    let gmp_api = Arc::new(gmp_api::GmpApi::new(&config.gmp_api_url, "xrpl").unwrap());

    let ingestor = Ingestor::new(gmp_api.clone(), config.clone());
    ingestor.run(events_queue, tasks_queue).await;
}
