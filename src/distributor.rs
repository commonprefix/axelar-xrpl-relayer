use std::sync::Arc;

use tokio::sync::watch;
use tracing::{info, warn};

use crate::{
    gmp_api::GmpApi,
    gmp_types::Task,
    queue::{Queue, QueueItem},
};

pub struct Distributor {
    last_task_id: i64,
}

impl Distributor {
    pub fn new() -> Self {
        Self { last_task_id: -1 } // TODO: should this be 0?
    }

    async fn work(&mut self, gmp_api: Arc<GmpApi>, queue: Arc<Queue>) -> () {
        let tasks_res = gmp_api.get_tasks_action(Some(self.last_task_id)).await;
        match tasks_res {
            Ok(tasks) => {
                for task in tasks {
                    let task_string =
                        serde_json::to_string(&QueueItem::Task(task.clone())).unwrap();
                    info!("Publishing task: {:?}", task);
                    queue.publish(task_string.as_bytes()).await;
                    let task_id = task.id();
                    self.last_task_id = std::cmp::max(self.last_task_id, task_id);
                }
            }
            Err(e) => {
                warn!("Error getting tasks: {:?}", e.to_string());
                warn!("Retrying in 2 seconds");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    pub async fn run(
        &mut self,
        gmp_api: Arc<GmpApi>,
        queue: Arc<Queue>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) -> () {
        loop {
            tokio::select! {
                _ = self.work(gmp_api.clone(), queue.clone()) => {}
                _ = shutdown_rx.changed() => {
                    info!("Shutting down distributor");
                    break;
                }
            }
        }
    }
}
