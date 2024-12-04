use std::{future::Future, pin::Pin, sync::Arc};

use anyhow::anyhow;
use async_stream::stream;
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};
use xrpl_api::{
    AccountTransaction, SubscribeRequest, Transaction, TransactionEvent, UnsubscribeRequest,
};
use xrpl_types::AccountId;
use xrpl_ws_client::client::TypedMessage;

use crate::queue::Queue;

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
        &self,
        account: AccountId,
    ) -> impl Future<Output = Result<Vec<Self::Transaction>, anyhow::Error>>;
}

pub enum Subscriber {
    Xrpl(XrplSubscriber),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ChainTransaction {
    Xrpl(TransactionEvent),
}

impl Subscriber {
    pub async fn new_xrpl(url: &str) -> Subscriber {
        let client = XrplSubscriber::new(url).await;
        Subscriber::Xrpl(client)
    }

    async fn work(&self, account: String, queue: Arc<Queue>) -> () {
        match self {
            Subscriber::Xrpl(sub) => {
                let res = sub.poll(AccountId::from_address(&account).unwrap()).await;
                match res {
                    Ok(txs) => {
                        for tx_with_meta in txs {
                            let tx_string = serde_json::to_string(&tx_with_meta.tx).unwrap();
                            info!("Publishing tx: {:?}", tx_string);
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

pub struct XrplWsSubscriber {
    client: xrpl_ws_client::client::Client,
}

impl XrplWsSubscriber {
    pub async fn new(url: &str) -> Self {
        let client = xrpl_ws_client::client::Client::connect(url)
            .await
            .expect("Failed to setup client");
        XrplWsSubscriber { client }
    }
}

impl TransactionListener for XrplWsSubscriber {
    type Transaction = ChainTransaction;

    async fn subscribe(&mut self, account: AccountId) -> Result<(), anyhow::Error> {
        let res = self
            .client
            .call(SubscribeRequest::accounts(vec![account.to_address()]))
            .await;
        if res.is_err() {
            return Err(anyhow!("Error subscribing: {:?}", res.err()));
        }
        Ok(())
    }

    async fn unsubscribe(&mut self, account: AccountId) -> Result<(), anyhow::Error> {
        let res = self
            .client
            .call(UnsubscribeRequest::accounts(vec![account.to_address()]))
            .await;
        if res.is_err() {
            return Err(anyhow!("Error unsubscribing: {:?}", res.err()));
        }
        Ok(())
    }

    // TODO: simulate connection close from remote and see what happens
    async fn transaction_stream(&mut self) -> Pin<Box<dyn Stream<Item = Self::Transaction> + '_>> {
        let mut messages = self.client.messages.as_mut();
        let s = stream! {
            while let Some(msg) = messages.next().await {
                match msg {
                    Ok(msg) => match msg {
                        TypedMessage::LedgerClosed(_) => {
                            todo!()
                        }
                        TypedMessage::AccountInfo(_) => {
                            todo!()
                        }
                        TypedMessage::Other(msg) => {
                            debug!("Received message: {:?}", msg);
                            let event_from_json = serde_json::from_str(msg.as_str());
                            if event_from_json.is_err() {
                                warn!("Failed to parse event: {:?}", event_from_json.err());
                            } else {
                                let event: TransactionEvent = event_from_json.unwrap();
                                match event.transaction {
                                    Transaction::Payment(_) |
                                    Transaction::SignerListSet(_) |
                                    Transaction::TrustSet(_) |
                                    Transaction::TicketCreate(_) => {
                                        debug!("Received tx: {:?}", event.transaction);
                                        yield ChainTransaction::Xrpl(event);
                                    }
                                    _ => {
                                        warn!("Received non-payment tx: {:?}", event.transaction);
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => {
                        error!("Error receiving message: {:?}", e);
                        break;
                    }
                }
            }
            warn!("Subscriber stream closed."); // TODO: should we handle?
        };
        Box::pin(s)
    }
}

pub struct XrplSubscriber {
    client: xrpl_http_client::Client,
}

impl XrplSubscriber {
    pub async fn new(url: &str) -> Self {
        // let client = Client::connect(url).await.expect("Failed to setup client");
        let client = xrpl_http_client::Client::builder().base_url(url).build();
        XrplSubscriber { client }
    }
}

impl TransactionPoller for XrplSubscriber {
    type Transaction = AccountTransaction;

    async fn poll(&self, account_id: AccountId) -> Result<Vec<Self::Transaction>, anyhow::Error> {
        let res = self
            .client
            .call(xrpl_api::AccountTxRequest::new(&account_id.to_address()))
            .await;

        let response = res.map_err(|e| anyhow!("Error getting txs: {:?}", e.to_string()))?;
        Ok(response.transactions)
    }
}
