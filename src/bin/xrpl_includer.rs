use dotenv::dotenv;
use std::sync::Arc;

use axelar_xrpl_relayer::{
    config::Config, gmp_api, queue::Queue, utils::setup_logging, xrpl::XrplIncluder,
};

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = Config::from_env().map_err(|e| anyhow::anyhow!(e)).unwrap();

    let _ = setup_logging(&config);

    let tasks_queue = Queue::new(&config.queue_address, "tasks").await;
    let gmp_api = Arc::new(gmp_api::GmpApi::new(&config.gmp_api_url, "xrpl").unwrap());
    let xrpl_includer = XrplIncluder::new(config.clone(), gmp_api.clone())
        .await
        .unwrap();

    xrpl_includer.run(tasks_queue).await;
}
