use std::sync::Arc;

use tokio::sync::watch;
use tracing::{info, warn};

use crate::{gmp_api::GmpApi, queue::Queue};

// add tasks from the amplifier to RMQ
pub struct Distributor {}

impl Distributor {
    async fn work(gmp_api: Arc<GmpApi>, queue: Arc<Queue>) -> () {
        let tasks_res = gmp_api.get_tasks_action().await;
        match tasks_res {
            Ok(tasks) => {
                for task in tasks {
                    let task_string = serde_json::to_string(&task).unwrap();
                    queue.publish(task_string.as_bytes()).await;
                }
            }
            Err(e) => {
                warn!("Error getting tasks: {:?}", e.to_string());
                warn!("Retrying in 2 seconds");
            }
        }
        // sleep 2 seconds
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    pub async fn run(
        gmp_api: Arc<GmpApi>,
        queue: Arc<Queue>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) -> () {
        loop {
            tokio::select! {
                _ = Distributor::work(gmp_api.clone(), queue.clone()) => {}
                _ = shutdown_rx.changed() => {
                    info!("Shutting down distributor");
                    break;
                }
            }
        }
    }
}
