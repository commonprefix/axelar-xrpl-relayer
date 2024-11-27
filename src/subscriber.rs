use std::{future::Future, pin::Pin};

use anyhow::anyhow;
use async_stream::stream;
use futures::{Stream, StreamExt};
use tracing::{debug, error, warn};
use xrpl_api::{SubscribeRequest, Transaction, TransactionEvent, UnsubscribeRequest};
use xrpl_types::AccountId;
use xrpl_ws_client::client::{Client, TypedMessage};

pub trait TransactionSubscriber {
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

pub enum Subscriber {
    Xrpl(XrplSubscriber),
}

#[derive(Debug)]
pub enum ChainTransaction {
    Xrpl(TransactionEvent),
}

impl Subscriber {
    pub async fn new_xrpl(url: &str) -> Subscriber {
        let client = XrplSubscriber::new(url).await;
        Subscriber::Xrpl(client)
    }

    pub async fn subscribe(&mut self, account: AccountId) -> Result<(), anyhow::Error> {
        match self {
            Subscriber::Xrpl(sub) => sub.subscribe(account).await,
        }
    }

    pub async fn unsubscribe(&mut self, accounts: AccountId) -> Result<(), anyhow::Error> {
        match self {
            Subscriber::Xrpl(sub) => sub.unsubscribe(accounts).await,
        }
    }

    pub async fn transaction_stream(
        &mut self,
    ) -> Pin<Box<dyn Stream<Item = ChainTransaction> + '_>> {
        match self {
            Subscriber::Xrpl(sub) => Box::pin(sub.transaction_stream().await),
        }
    }
}

pub struct XrplSubscriber {
    client: Client,
}

impl XrplSubscriber {
    pub async fn new(url: &str) -> XrplSubscriber {
        let client = Client::connect(url).await.expect("Failed to setup client");
        XrplSubscriber { client }
    }
}

impl TransactionSubscriber for XrplSubscriber {
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
