use std::{future::Future, sync::Arc};

use futures::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicNackOptions},
    Consumer,
};
use tokio::sync::watch;
use tracing::{debug, error, info};

use crate::{
    error::{BroadcasterError, IncluderError, RefundManagerError},
    gmp_api::GmpApi,
    gmp_types::{Amount, CommonEventFields, Event, Task},
    queue::{Queue, QueueItem},
};

pub trait RefundManager {
    fn build_refund_tx(
        &self,
        recipient: String,
        amount: String,
    ) -> impl Future<Output = Result<String, RefundManagerError>>; // returns signed tx_blob
}

pub trait Broadcaster {
    fn broadcast(&self, tx_blob: String) -> impl Future<Output = Result<String, BroadcasterError>>;
}

pub struct Includer<B, C, R>
where
    B: Broadcaster,
    R: RefundManager,
{
    pub chain_client: C,
    pub broadcaster: B,
    pub refund_manager: R,
    pub gmp_api: Arc<GmpApi>,
}

impl<B, C, R> Includer<B, C, R>
where
    B: Broadcaster,
    R: RefundManager,
{
    async fn work(&self, consumer: &mut Consumer) -> () {
        let next_item = consumer.next().await;

        if let Some(delivery) = next_item {
            match delivery {
                Ok(delivery) => {
                    let data = delivery.data.clone();
                    let task = serde_json::from_slice::<QueueItem>(&data).unwrap();

                    let consume_res = self.consume(task).await;
                    match consume_res {
                        Ok(_) => {
                            info!("Successfully consumed delivery");
                            delivery.ack(BasicAckOptions::default()).await.expect("ack");
                        }
                        Err(e) => {
                            match e {
                                IncluderError::IrrelevantTask => {
                                    debug!("Skipping irrelevant task");
                                }
                                _ => {
                                    error!("Failed to consume delivery: {:?}", e);
                                }
                            }

                            delivery
                                .nack(BasicNackOptions {
                                    multiple: false,
                                    requeue: true,
                                })
                                .await
                                .expect("nack");
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to receive delivery: {:?}", e);
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await
    }

    pub async fn run(&self, queue: Arc<Queue>, mut shutdown_rx: watch::Receiver<bool>) -> () {
        let queue = queue.clone();
        let mut consumer = queue.consumer().await.unwrap();
        loop {
            tokio::select! {
                _ = self.work(&mut consumer) => {}
                _ = shutdown_rx.changed() => {
                    info!("Shutting down includer");
                    break;
                }
            }
        }
    }

    pub async fn consume(&self, task: QueueItem) -> Result<(), IncluderError> {
        info!("Consuming task: {:?}", task);
        match task {
            QueueItem::Task(task) => match task {
                Task::GatewayTx(gateway_tx_task) => {
                    let broadcast_result = self
                        .broadcaster
                        .broadcast(gateway_tx_task.task.execute_data)
                        .await
                        .map_err(|e| IncluderError::ConsumerError(e.to_string()));

                    if broadcast_result.is_err() {
                        let cannot_execute_message_event = Event::CannotExecuteMessage {
                            common: CommonEventFields {
                                r#type: "CANNOT_EXECUTE_MESSAGE".to_owned(),
                                event_id: "foobar".to_owned(), // TODO
                            },
                            task_item_id: gateway_tx_task.common.id.to_string(),
                            reason: broadcast_result.unwrap_err().to_string(),
                            details: Amount {
                                // TODO
                                token_id: None,
                                amount: "0".to_owned(),
                            },
                        };

                        self.gmp_api
                            .post_events(vec![cannot_execute_message_event])
                            .await
                            .map_err(|e| IncluderError::ConsumerError(e.to_string()))?;
                    }
                    Ok(())
                }
                Task::Refund(refund_task) => {
                    let tx_blob = self
                        .refund_manager
                        .build_refund_tx(
                            refund_task.task.refund_recipient_address,
                            refund_task.task.remaining_gas_balance.amount, // TODO: check if this is correct
                        )
                        .await
                        .map_err(|e| IncluderError::ConsumerError(e.to_string()))?;
                    self.broadcaster
                        .broadcast(tx_blob)
                        .await
                        .map_err(|e| IncluderError::ConsumerError(e.to_string()))?;
                    Ok(())
                }
                _ => Err(IncluderError::IrrelevantTask),
            },
            _ => Err(IncluderError::GenericError(
                "Invalid queue item".to_string(),
            )),
        }
    }
}
