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
        let http_client = reqwest::ClientBuilder::new()
            .connect_timeout(DEFAULT_RPC_TIMEOUT)
            .timeout(DEFAULT_RPC_TIMEOUT)
            .build()
            .map_err(|e| ClientError::ConnectionFailed(e.to_string()))?;

        Ok(xrpl_http_client::Client::builder()
            .base_url(url)
            .http_client(http_client)
            .build())
    }
}

pub struct XRPLRefundManager {
    client: Arc<xrpl_http_client::Client>,
    account_id: AccountId,
    secret_key: SecretKey,
    public_key: PublicKey,
}

impl<'a> XRPLRefundManager {
    fn new(
        client: Arc<xrpl_http_client::Client>,
        address: String,
        secret: String,
    ) -> Result<Self, RefundManagerError> {
        let account_id = AccountId::from_address(&address)
            .map_err(|e| RefundManagerError::GenericError(format!("Invalid address: {}", e)))?;

        let secret_bytes = hex::decode(&secret)
            .map_err(|e| RefundManagerError::GenericError(format!("Hex decode error: {}", e)))?;

        let secret_key = SecretKey::parse_slice(&secret_bytes).map_err(|err| {
            RefundManagerError::GenericError(format!("Invalid secret key: {:?}", err))
        })?;

        let public_key = PublicKey::from_secret_key(&secret_key);

        debug!("Creating refund manager with address: {}", address);
        Ok(Self {
            client,
            account_id,
            secret_key,
            public_key,
        })
    }
}

impl RefundManager for XRPLRefundManager {
    async fn build_refund_tx(
        &self,
        recipient: String,
        drops: String,
    ) -> Result<String, RefundManagerError> {
        let drops_u64 = drops.parse::<u64>().map_err(|e| {
            RefundManagerError::GenericError(format!("Invalid drops amount '{}': {}", drops, e))
        })?;

        let amount = Amount::drops(drops_u64).map_err(|e| {
            RefundManagerError::GenericError(format!("Failed to parse amount: {}", e.to_string()))
        })?;

        let recipient_account = AccountId::from_address(&recipient).map_err(|e| {
            RefundManagerError::GenericError(format!("Invalid recipient address: {}", e))
        })?;

        let mut tx = PaymentTransaction::new(self.account_id, amount, recipient_account);

        self.client
            .prepare_transaction(&mut tx.common)
            .await
            .map_err(|e| RefundManagerError::GenericError(e.to_string()))?;

        sign_transaction(&mut tx, &self.public_key, &self.secret_key)
            .map_err(|e| RefundManagerError::GenericError(format!("Sign error: {}", e)))?;

        let tx_bytes = serialize::serialize(&tx)
            .map_err(|e| RefundManagerError::GenericError(format!("Serialization error: {}", e)))?;

        Ok(hex::encode_upper(tx_bytes))
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
        let response = self
            .client
            .call(req)
            .await
            .map_err(|e| BroadcasterError::RPCCallFailed(e.to_string()))?;

        serde_json::to_string(&response).map_err(|e| {
            BroadcasterError::RPCCallFailed(format!("JSON serialization error: {}", e))
        })
    }
}

pub struct XrplIncluder {}

impl XrplIncluder {
    pub async fn new<'a>(
        config: Config,
        gmp_api: Arc<GmpApi>,
    ) -> error_stack::Result<
        Includer<XRPLBroadcaster, Arc<xrpl_http_client::Client>, XRPLRefundManager>,
        BroadcasterError,
    > {
        let client =
            Arc::new(XRPLClient::new_http_client(RPC_URL).map_err(|e| {
                error_stack::report!(BroadcasterError::GenericError(e.to_string()))
            })?);

        let broadcaster = XRPLBroadcaster::new(Arc::clone(&client))
            .map_err(|e| e.attach_printable("Failed to create XRPLBroadcaster"))?;

        let refund_manager = XRPLRefundManager::new(
            Arc::clone(&client),
            config.refund_manager_address,
            config.includer_secret,
        )
        .map_err(|e| error_stack::report!(BroadcasterError::GenericError(e.to_string())))?;

        let includer = Includer {
            chain_client: client,
            broadcaster,
            refund_manager,
            gmp_api,
        };

        Ok(includer)
    }
}
