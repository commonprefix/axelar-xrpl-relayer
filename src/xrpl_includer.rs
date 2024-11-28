use libsecp256k1::{PublicKey, SecretKey};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use xrpl_api::SubmitRequest;
use xrpl_binary_codec::serialize;
use xrpl_binary_codec::sign::sign_transaction;
use xrpl_types::PaymentTransaction;
use xrpl_types::{AccountId, Amount};

use crate::error::{BroadcasterError, ClientError, RefundManagerError};
use crate::includer::{Broadcaster, Includer, RefundManager};
use crate::queue::Queue;

const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(3);
const RPC_URL: &str = "https://s.devnet.rippletest.net:51234";

pub struct XRPLClient {}

impl XRPLClient {
    pub fn new_http_client(url: &str) -> Result<xrpl_http_client::Client, ClientError> {
        let client = xrpl_http_client::Client::builder()
            .base_url(url)
            .http_client(
                reqwest::ClientBuilder::new()
                    .connect_timeout(DEFAULT_RPC_TIMEOUT.into())
                    .timeout(DEFAULT_RPC_TIMEOUT)
                    .build()
                    .map_err(|e| ClientError::ConnectionFailed(e.to_string()))?,
            )
            .build();
        Ok(client)
    }
}

pub struct XRPLRefundManager {
    client: Arc<xrpl_http_client::Client>,
    account_id: AccountId,
    secret: String,
}

impl<'a> XRPLRefundManager {
    fn new(client: Arc<xrpl_http_client::Client>, address: String, secret: String) -> Self {
        Self {
            client,
            account_id: AccountId::from_address(&address).unwrap(),
            secret,
        }
    }
}

impl RefundManager for XRPLRefundManager {
    async fn build_refund_tx(
        &self,
        recipient: String,
        drops: u64,
    ) -> Result<String, RefundManagerError> {
        let mut tx = PaymentTransaction::new(
            self.account_id,
            Amount::drops(drops).unwrap(),
            AccountId::from_address(&recipient).unwrap(),
        );

        self.client
            .prepare_transaction(&mut tx.common)
            .await
            .map_err(|e| RefundManagerError::GenericError(e.to_string()))?;

        println!("{}", hex::decode(self.secret.clone()).unwrap().len());
        let secret_key =
            SecretKey::parse_slice(&hex::decode(self.secret.clone()).unwrap()).unwrap();
        let public_key = PublicKey::from_secret_key(&secret_key);
        sign_transaction(&mut tx, &public_key, &secret_key).unwrap();

        Ok(hex::encode_upper(serialize::serialize(&tx).unwrap()))
    }
}

pub struct XRPLBroadcaster {
    client: Arc<xrpl_http_client::Client>,
}

impl XRPLBroadcaster {
    fn new(client: Arc<xrpl_http_client::Client>) -> error_stack::Result<Self, BroadcasterError> {
        Ok(XRPLBroadcaster { client })
    }
}

impl Broadcaster for XRPLBroadcaster {
    async fn broadcast(&self, tx_blob: String) -> Result<String, BroadcasterError> {
        let req = SubmitRequest::new(tx_blob);
        let resp = self
            .client
            .call(req)
            .await
            .map_err(|e| BroadcasterError::RPCCallFailed(e.to_string()))?;

        Ok(serde_json::to_string(&resp).unwrap())
    }
}

pub struct XRPLIncluder {}

impl XRPLIncluder {
    pub async fn new<'a>(
    ) -> Includer<XRPLBroadcaster, Arc<xrpl_http_client::Client>, XRPLRefundManager> {
        let addr = "amqp://127.0.0.1:5672";
        let q = Queue::new(addr).await;

        let client = Arc::new(XRPLClient::new_http_client(RPC_URL).unwrap());

        let broadcaster = XRPLBroadcaster::new(Arc::clone(&client)).unwrap();
        let refund_manager = XRPLRefundManager::new(
            Arc::clone(&client),
            env::var("ADDRESS")
                .expect("ADDRESS environment variable")
                .to_string(),
            env::var("SECRET")
                .expect("SECRET environment variable")
                .to_string(),
        );

        let includer = Includer {
            queue: q,
            chain_client: client,
            broadcaster,
            refund_manager,
        };

        includer
    }
}
