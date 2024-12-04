use std::{future::Future, sync::Arc};

use futures::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicNackOptions},
    Consumer,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tracing::{error, info};

// parse RMQ events (amplifier tasks) and post txs XRPL chain
use crate::{
    error::{BroadcasterError, IncluderError, RefundManagerError},
    queue::Queue,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct RefundInfo {
    recipient: String,
    amount: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProverTx {
    tx_blob: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum IncluderMessage {
    Refund(RefundInfo),
    Prover(ProverTx),
}

pub trait RefundManager {
    fn build_refund_tx(
        &self,
        recipient: String,
        amount: u64,
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
                    let includer_msg = serde_json::from_slice::<IncluderMessage>(&data).unwrap();

                    let consume_res = self.consume(includer_msg).await;
                    match consume_res {
                        Ok(_) => {
                            info!("Successfully consumed delivery");
                            delivery.ack(BasicAckOptions::default()).await.expect("ack");
                        }
                        Err(e) => {
                            error!("Failed to consume delivery: {:?}", e);
                            delivery
                                .nack(BasicNackOptions::default())
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

    pub async fn consume(&self, msg: IncluderMessage) -> Result<(), IncluderError> {
        match msg {
            IncluderMessage::Refund(refund_info) => {
                let tx_blob = self
                    .refund_manager
                    .build_refund_tx(refund_info.recipient, refund_info.amount)
                    .await
                    .unwrap();
                let res = self.broadcaster.broadcast(tx_blob).await.unwrap();
                println!("{:?}", res);
            }
            IncluderMessage::Prover(prover_tx) => {
                let res = self.broadcaster.broadcast(prover_tx.tx_blob).await.unwrap();
                println!("{:?}", res);
            }
        }

        Ok(())
    }
}
