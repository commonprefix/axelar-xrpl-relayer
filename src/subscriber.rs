use std::{future::Future, pin::Pin, sync::Arc};

use futures::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};
use xrpl_api::Transaction;
use xrpl_types::AccountId;

use crate::{
    queue::{Queue, QueueItem},
    xrpl_subscriber::XrplSubscriber,
};

pub trait TransactionListener {
    type Transaction;

    fn subscribe(&mut self, account: AccountId) -> impl Future<Output = Result<(), anyhow::Error>>;

    fn unsubscribe(
        &mut self,
        accounts: AccountId,
    ) -> impl Future<Output = Result<(), anyhow::Error>>;

    fn transaction_stream(
        &mut self,
    ) -> impl Future<Output = Pin<Box<dyn Stream<Item = Self::Transaction> + '_>>>;
}

pub trait TransactionPoller {
    type Transaction;

    fn poll(
        &mut self,
        account: AccountId,
    ) -> impl Future<Output = Result<Vec<Self::Transaction>, anyhow::Error>>;
}

pub enum Subscriber {
    Xrpl(XrplSubscriber),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ChainTransaction {
    Xrpl(Transaction),
}

impl Subscriber {
    pub async fn new_xrpl(url: &str) -> Subscriber {
        let client = XrplSubscriber::new(url).await;
        Subscriber::Xrpl(client)
    }

    async fn work(&mut self, account: String, queue: Arc<Queue>) -> () {
        match self {
            Subscriber::Xrpl(sub) => {
                let res = sub.poll(AccountId::from_address(&account).unwrap()).await;
                match res {
                    Ok(txs) => {
                        for tx_with_meta in txs {
                            let chain_transaction = ChainTransaction::Xrpl(tx_with_meta.tx.clone());
                            let tx_string = serde_json::to_string(&QueueItem::Transaction(
                                chain_transaction.clone(),
                            ))
                            .unwrap();
                            info!("Publishing tx: {:?}", chain_transaction);
                            queue.publish(tx_string.as_bytes()).await;
                            debug!("Published tx: {:?}", tx_string);
                        }
                    }
                    Err(e) => {
                        error!("Error getting txs: {:?}", e);
                        warn!("Retrying in 2 seconds");
                    }
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await
    }

    pub async fn run(
        &mut self,
        account: String,
        queue: Arc<Queue>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) -> () {
        loop {
            tokio::select! {
                _ = self.work(account.clone(), queue.clone()) => {}
                _ = shutdown_rx.changed() => {
                    info!("Shutting down subscriber");
                    break;
                }
            }
        }
    }
}
