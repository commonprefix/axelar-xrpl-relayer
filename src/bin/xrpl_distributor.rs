use dotenv::dotenv;
use std::sync::Arc;

use axelar_xrpl_relayer::{
    config::Config, distributor::Distributor, gmp_api, queue::Queue, utils::setup_logging,
};

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = Config::from_env().map_err(|e| anyhow::anyhow!(e)).unwrap();

    setup_logging(&config);

    let tasks_queue = Queue::new(&config.queue_address, "tasks").await;
    let gmp_api = Arc::new(gmp_api::GmpApi::new(&config.gmp_api_url, "xrpl").unwrap());
    let redis_client = redis::Client::open(config.redis_server.clone()).unwrap();
    let redis_pool = r2d2::Pool::builder().build(redis_client).unwrap();

    let mut distributor = Distributor::new(redis_pool.clone());
    distributor.run(gmp_api, tasks_queue).await;
}
