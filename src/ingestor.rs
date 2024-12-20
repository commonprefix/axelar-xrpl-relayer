use core::str;
use std::sync::Arc;

use futures::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicNackOptions},
    Consumer,
};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::{
    config::Config,
    error::IngestorError,
    gmp_api::GmpApi,
    gmp_types::Task,
    queue::{Queue, QueueItem},
    subscriber::ChainTransaction,
    xrpl_ingestor::XrplIngestor,
};

pub struct Ingestor {
    gmp_api: Arc<GmpApi>,
    xrpl_ingestor: XrplIngestor,
}

impl Ingestor {
    pub fn new(gmp_api: Arc<GmpApi>, config: Config) -> Self {
        let xrpl_ingestor = XrplIngestor::new(gmp_api.clone(), config.clone());
        Self {
            gmp_api,
            xrpl_ingestor,
        }
    }

    async fn work(&self, consumer: &mut Consumer) -> () {
        match consumer.next().await {
            Some(Ok(delivery)) => {
                if let Err(e) = self.process_delivery(&delivery.data).await {
                    match e {
                        IngestorError::IrrelevantTask => {
                            debug!("Skipping irrelevant task");
                        }
                        _ => {
                            error!("Failed to consume delivery: {:?}", e);
                        }
                    }

                    if let Err(nack_err) = delivery
                        .nack(BasicNackOptions {
                            multiple: false,
                            requeue: true,
                        })
                        .await
                    {
                        error!("Failed to nack message: {:?}", nack_err);
                    }
                } else if let Err(ack_err) = delivery.ack(BasicAckOptions::default()).await {
                    error!("Failed to ack message: {:?}", ack_err);
                }
            }
            Some(Err(e)) => {
                error!("Failed to receive delivery: {:?}", e);
            }
            None => {
                //TODO:  Consumer stream ended. Possibly handle reconnection logic here if needed.
                warn!("No more messages from consumer.");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await
    }

    pub async fn run(
        &self,
        events_queue: Arc<Queue>,
        tasks_queue: Arc<Queue>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) -> () {
        let mut events_consumer = events_queue.consumer().await.unwrap();
        let mut tasks_consumer = tasks_queue.consumer().await.unwrap();

        loop {
            tokio::select! {
                _ = self.work(&mut events_consumer) => {}
                _ = self.work(&mut tasks_consumer) => {}
                _ = shutdown_rx.changed() => {
                    info!("Shutting down ingestor");
                    break;
                }
            }
        }
    }

    async fn process_delivery(&self, data: &[u8]) -> Result<(), IngestorError> {
        let data_str = str::from_utf8(&data)
            .map_err(|e| IngestorError::ParseError(format!("Invalid UTF-8 data: {}", e)))?;
        debug!("Received data string: {}", data_str);

        let item = serde_json::from_slice::<QueueItem>(&data)
            .map_err(|e| IngestorError::ParseError(format!("Invalid JSON: {}", e)))?;

        self.consume(item).await
    }

    pub async fn consume(&self, item: QueueItem) -> Result<(), IngestorError> {
        info!("Consuming item: {:?}", item);
        match item {
            QueueItem::Task(task) => self.consume_task(task).await,
            QueueItem::Transaction(chain_transaction) => {
                self.consume_transaction(chain_transaction).await
            }
        }
    }

    pub async fn consume_transaction(
        &self,
        transaction: ChainTransaction,
    ) -> Result<(), IngestorError> {
        let events = match transaction {
            ChainTransaction::Xrpl(tx) => self.xrpl_ingestor.handle_transaction(tx).await?,
        };

        info!("Posting events: {:?}", events.clone());
        let response = self
            .gmp_api
            .post_events(events)
            .await
            .map_err(|e| IngestorError::PostEventError(e.to_string()))?;

        for event_response in response {
            if event_response.status != "ACCEPTED" {
                error!("Posting event failed: {:?}", event_response.error.clone());
                if event_response.retriable.is_some() && event_response.retriable.unwrap() {
                    return Err(IngestorError::RetriableError(
                        // TODO: retry? Handle error responses for part of the batch
                        // Question: what happens if we send the same event multiple times?
                        event_response.error.clone().unwrap_or_default(),
                    ));
                }
            }
        }
        Ok(()) // TODO: better error handling
    }

    pub async fn consume_task(&self, task: Task) -> Result<(), IngestorError> {
        match task {
            Task::Verify(verify_task) => self.xrpl_ingestor.handle_verify(verify_task).await,
            Task::ReactToWasmEvent(react_to_wasm_event_task) => {
                self.xrpl_ingestor
                    .handle_wasm_event(react_to_wasm_event_task)
                    .await
            }
            Task::ConstructProof(construct_proof_task) => {
                self.xrpl_ingestor
                    .handle_construct_proof(construct_proof_task)
                    .await
            }
            Task::Refund(_) => todo!(),
            _ => Err(IngestorError::IrrelevantTask),
        }
    }
}
