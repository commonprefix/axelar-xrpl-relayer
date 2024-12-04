use core::str;
use std::sync::Arc;

use futures::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicNackOptions},
    Consumer,
};
use tokio::sync::watch;
use tracing::{error, info, warn};
use xrpl_api::Transaction;

use crate::{
    error::IngestorError,
    gmp_api::GmpApi,
    gmp_types::{CallEvent, CommonEventFields, Event, Message},
    queue::Queue,
    subscriber::ChainTransaction,
};

// parse messages from RMQ (coming from XRPL) post Amplifier events
pub struct Ingestor {}

impl Ingestor {
    async fn work(gmp_api: Arc<GmpApi>, consumer: &mut Consumer) -> () {
        let next_item = consumer.next().await;
        if let Some(delivery) = next_item {
            match delivery {
                Ok(delivery) => {
                    let data = delivery.data.clone();
                    let tx = serde_json::from_slice::<ChainTransaction>(&data).unwrap();
                    let consume_res = Ingestor::consume(gmp_api, tx).await;
                    match consume_res {
                        Ok(_) => {
                            delivery.ack(BasicAckOptions::default()).await.expect("ack");
                        }
                        Err(e) => {
                            warn!("Error consuming tx: {:?}", e);
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
    }

    pub async fn run(
        gmp_api: Arc<GmpApi>,
        queue: Arc<Queue>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) -> () {
        let mut consumer = queue.consumer().await.unwrap();

        loop {
            tokio::select! {
                _ = Ingestor::work(gmp_api.clone(), &mut consumer) => {}
                _ = shutdown_rx.changed() => {
                    info!("Shutting down ingestor");
                    break;
                }
            }
        }
    }

    pub async fn consume(gmp_api: Arc<GmpApi>, tx: ChainTransaction) -> Result<(), IngestorError> {
        match tx {
            ChainTransaction::Xrpl(tx_event) => {
                let tx = tx_event.transaction;
                match tx {
                    Transaction::Payment(payment) => {
                        let event = Event::Call(CallEvent {
                            common: CommonEventFields {
                                r#type: "CALL".to_owned(),
                                event_id: "myeventid".to_owned(),
                            },
                            message: Message {
                                message_id: "message_id".to_owned(), // TODO
                                source_chain: "xrpl".to_owned(),
                                source_address: payment.common.account,
                                destination_address: str::from_utf8(
                                    hex::decode(
                                        payment.common.memos.clone().unwrap()[0]
                                            .memo_data
                                            .clone()
                                            .unwrap(),
                                    )
                                    .unwrap()
                                    .as_slice(),
                                )
                                .unwrap()
                                .to_string(),
                                payload_hash: payment.common.memos.clone().unwrap()[2]
                                    .memo_data
                                    .clone()
                                    .unwrap(),
                            },
                            destination_chain: str::from_utf8(
                                hex::decode(
                                    payment.common.memos.clone().unwrap()[1]
                                        .memo_data
                                        .clone()
                                        .unwrap(),
                                )
                                .unwrap()
                                .as_slice(),
                            )
                            .unwrap()
                            .to_string(),
                            payload: "".to_owned(),
                            meta: None,
                        });

                        info!("Posting event: {:?}", event);
                        // TODO: batching
                        let res = gmp_api
                            .post_events(vec![event.clone()])
                            .await
                            .map_err(|e| IngestorError::PostEventError(e.to_string()))?;
                        let res = res.get(0).unwrap();

                        if res.status != "ACCEPTED" {
                            error!("Posting event failed: {:?}", res.error.clone());
                            if res.retriable.is_some() && res.retriable.unwrap() {
                                return Err(IngestorError::RetriableError(
                                    res.error.clone().unwrap_or_default(),
                                ));
                            }
                        }
                    }
                    Transaction::TrustSet(_) => {
                        todo!()
                    }
                    Transaction::SignerListSet(_) => {
                        todo!()
                    }
                    Transaction::TicketCreate(_) => {
                        todo!()
                    }
                    _ => {
                        warn!("Received non-payment tx: {:?}", tx);
                    }
                }
                Ok(())
            }
        }
    }
}
