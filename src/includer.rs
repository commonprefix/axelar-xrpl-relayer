use futures::StreamExt;
use lapin::options::BasicAckOptions;
use serde::{Deserialize, Serialize};

// parse RMQ events (amplifier tasks) and post txs XRPL chain
use crate::{
    error::{BroadcasterError, IncluderError, RefundManagerError},
    queue::Queue,
};

#[derive(Debug, Serialize, Deserialize)]
struct RefundInfo {
    recipient: String,
    amount: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProverTx {
    tx_blob: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum IncluderMessage {
    Refund(RefundInfo),
    Prover(ProverTx),
}

pub trait RefundManager {
    async fn build_refund_tx(
        &self,
        recipient: String,
        amount: u64,
    ) -> Result<String, RefundManagerError>; // returns signed tx_blob
}

pub trait Broadcaster {
    async fn broadcast(&self, tx_blob: String) -> Result<String, BroadcasterError>;
}

pub struct Includer<B, C, R>
where
    B: Broadcaster,
    R: RefundManager,
{
    pub queue: Queue,
    pub chain_client: C,
    pub broadcaster: B,
    pub refund_manager: R,
}

impl<B, C, R> Includer<B, C, R>
where
    B: Broadcaster,
    R: RefundManager,
{
    pub async fn run(&self) -> () {
        let mut queue = self.queue.clone();
        let consumer = queue.consumer().await.unwrap();
        while let Some(delivery) = consumer.next().await {
            let delivery = delivery.expect("error in consumer");

            let data = delivery.data.clone();
            let includer_msg = serde_json::from_slice::<IncluderMessage>(&data).unwrap();
            self.consume(includer_msg).await;
            delivery.ack(BasicAckOptions::default()).await.expect("ack");
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
