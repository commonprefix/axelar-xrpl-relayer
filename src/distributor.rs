use std::{default, sync::Arc};

use r2d2::{Pool, PooledConnection};
use redis::Commands;
use tokio::sync::{watch, RwLockReadGuard};
use tracing::{debug, info, warn};

use crate::{
    error::DistributorError,
    gmp_api::GmpApi,
    queue::{Queue, QueueItem},
};

pub struct Distributor {
    redis_conn: PooledConnection<redis::Client>,
    last_task_id: i64,
}

impl Distributor {
    pub fn new(redis_pool: Pool<redis::Client>) -> Self {
        let mut redis_conn = redis_pool
            .get()
            .expect("Cannot get redis connection from pool");

        let last_task_id = redis_conn.get("distributor:last_task_id").unwrap_or(-1);
        if last_task_id != -1 {
            info!("Distributor: recovering last task id: {}", last_task_id);
        }

        Self {
            redis_conn,
            last_task_id,
        }
    }

    async fn store_last_task_id(&mut self) -> Result<(), DistributorError> {
        let _: () = self
            .redis_conn
            .set("distributor:last_task_id", self.last_task_id)
            .map_err(|e| {
                DistributorError::GenericError(format!(
                    "Failed to store last_task_id to Redis: {}",
                    e.to_string()
                ))
            })?;
        Ok(())
    }

    async fn work(&mut self, gmp_api: Arc<GmpApi>, queue: Arc<Queue>) -> () {
        let tasks_res = gmp_api.get_tasks_action(Some(self.last_task_id)).await;
        match tasks_res {
            Ok(tasks) => {
                for task in tasks {
                    let task_item = &QueueItem::Task(task.clone());
                    info!("Publishing task: {:?}", task);
                    queue.publish(task_item.clone()).await;
                    let task_id = task.id();
                    self.last_task_id = std::cmp::max(self.last_task_id, task_id);

                    if let Err(err) = self.store_last_task_id().await {
                        warn!("{:?}", err);
                    }
                }
            }
            Err(e) => {
                warn!("Error getting tasks: {:?}", e.to_string());
                debug!("Retrying in 2 seconds");
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
            info!("Distributor is alive.");
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
