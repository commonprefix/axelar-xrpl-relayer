use dotenv::dotenv;
use std::sync::Arc;

use axelar_xrpl_relayer::{config::Config, gmp_api, utils::setup_logging, xrpl::XrplTicketCreator};

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = Config::from_env().map_err(|e| anyhow::anyhow!(e)).unwrap();

    let _guard = setup_logging(&config);

    let gmp_api = Arc::new(gmp_api::GmpApi::new(&config.gmp_api_url, "xrpl").unwrap());
    let ticket_creator = XrplTicketCreator::new(gmp_api.clone(), config.clone());
    ticket_creator.run().await;
}
