use libsecp256k1::{PublicKey, SecretKey};
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;
use xrpl_api::SubmitRequest;
use xrpl_binary_codec::serialize;
use xrpl_binary_codec::sign::sign_transaction;
use xrpl_types::PaymentTransaction;
use xrpl_types::{AccountId, Amount};

use crate::config::Config;
use crate::error::{BroadcasterError, ClientError, RefundManagerError};
use crate::gmp_api::GmpApi;
use crate::includer::{Broadcaster, Includer, RefundManager};

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
        debug!("Creating refund manager with address: {}", address);
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
        drops: String,
    ) -> Result<String, RefundManagerError> {
        let mut tx = PaymentTransaction::new(
            self.account_id,
            Amount::drops(drops.parse::<u64>().unwrap()).unwrap(), // TODO: could this overflow?
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
        config: Config,
        gmp_api: Arc<GmpApi>,
    ) -> Includer<XRPLBroadcaster, Arc<xrpl_http_client::Client>, XRPLRefundManager> {
        let client = Arc::new(XRPLClient::new_http_client(RPC_URL).unwrap());

        let broadcaster = XRPLBroadcaster::new(Arc::clone(&client)).unwrap();
        let refund_manager = XRPLRefundManager::new(
            Arc::clone(&client),
            config.refund_manager_address,
            config.includer_secret,
        );

        let includer = Includer {
            chain_client: client,
            broadcaster,
            refund_manager,
            gmp_api,
        };

        includer
    }
}
