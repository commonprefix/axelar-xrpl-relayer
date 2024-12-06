use core::str;
use std::sync::Arc;

use futures::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicNackOptions},
    Consumer,
};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};
use xrpl_api::Transaction;

use crate::{
    error::IngestorError,
    gmp_api::GmpApi,
    gmp_types::{CallEvent, CommonEventFields, Event, Message, Task},
    queue::{Queue, QueueItem},
    subscriber::ChainTransaction,
};

pub struct Ingestor {}

impl Ingestor {
    async fn work(gmp_api: Arc<GmpApi>, consumer: &mut Consumer, multisig_address: String) -> () {
        let next_item = consumer.next().await;
        if let Some(delivery) = next_item {
            match delivery {
                Ok(delivery) => {
                    let data = delivery.data.clone();
                    debug!("Received data string: {:?}", str::from_utf8(&data).unwrap());
                    let item = serde_json::from_slice::<QueueItem>(&data).unwrap();
                    let consume_res = Ingestor::consume(gmp_api, item, multisig_address).await;
                    match consume_res {
                        Ok(_) => {
                            delivery.ack(BasicAckOptions::default()).await.expect("ack");
                        }
                        Err(e) => {
                            warn!("Error consuming tx: {:?}", e);
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

    pub async fn run(
        gmp_api: Arc<GmpApi>,
        events_queue: Arc<Queue>,
        tasks_queue: Arc<Queue>,
        multisig_address: String,
        mut shutdown_rx: watch::Receiver<bool>,
    ) -> () {
        let mut events_consumer = events_queue.consumer().await.unwrap();
        let mut tasks_consumer = tasks_queue.consumer().await.unwrap();

        loop {
            tokio::select! {
                _ = Ingestor::work(gmp_api.clone(), &mut events_consumer, multisig_address.clone()) => {}
                _ = Ingestor::work(gmp_api.clone(), &mut tasks_consumer, multisig_address.clone()) => {}
                _ = shutdown_rx.changed() => {
                    info!("Shutting down ingestor");
                    break;
                }
            }
        }
    }

    pub async fn consume(
        gmp_api: Arc<GmpApi>,
        item: QueueItem,
        multisig_address: String,
    ) -> Result<(), IngestorError> {
        match item {
            QueueItem::Task(task) => match task {
                Task::Verify(verify_task) => todo!(),
                Task::ConstructProof(construct_proof_task) => todo!(),
                Task::ReactToWasmEvent(react_to_wasm_event_task) => todo!(),
                Task::Execute(_) => Err(IngestorError::IrrelevantTask),
                Task::GatewayTx(_) => Err(IngestorError::IrrelevantTask),
                Task::Refund(_) => Err(IngestorError::IrrelevantTask),
            },
            QueueItem::Transaction(chain_transaction) => {
                match chain_transaction {
                    ChainTransaction::Xrpl(tx) => {
                        // TODO: query the gateway on Axelar to get the ITS Hub message
                        match tx {
                            Transaction::Payment(payment) => {
                                if payment.destination != multisig_address {
                                    return Ok(());
                                }

                                // TODO: also create GAS_CREDIT event
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
    }
}
