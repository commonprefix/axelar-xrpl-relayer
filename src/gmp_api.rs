use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

use reqwest::Client;

use crate::error::GmpApiError;

pub struct GmpApi {
    client: Client,
}

const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(3);
const RPC_URL: &str = "https://s.devnet.rippletest.net:51234";

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    messageID: String,
    sourceChain: String,
    sourceAddress: String,
    destinationAddress: String,
    payloadHash: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Amount {
    tokenID: Option<String>,
    amount: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct CommonTaskFields {
    id: String,
    timestamp: String,
    r#type: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct ExecuteTaskFields {
    message: Message,
    payload: String,
    availableGasBalance: Amount,
}

#[derive(Serialize, Deserialize, Debug)]
struct ExecuteTask {
    #[serde(flatten)]
    common: CommonTaskFields,
    task: ExecuteTaskFields,
}

#[derive(Serialize, Deserialize, Debug)]
struct RefundTaskFields {
    message: Message,
    refundRecipientAddress: String,
    remainingGasBalance: Amount,
}

#[derive(Serialize, Deserialize, Debug)]
struct RefundTask {
    #[serde(flatten)]
    common: CommonTaskFields,
    task: RefundTaskFields,
}

#[derive(Serialize, Deserialize, Debug)]
struct CommonEventFields {
    r#type: String,
    eventID: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Metadata {
    txID: String,
    fromAddress: String,
    finalized: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct CallEvent {
    #[serde(flatten)]
    common: CommonEventFields,
    message: Message,
    destinationChain: String,
    payload: String,
    meta: Option<Metadata>,
}

#[derive(Serialize, Deserialize, Debug)]
struct GasRefundedEvent {
    #[serde(flatten)]
    common: CommonEventFields,
    recipientAddress: String,
    refundedAmount: Amount,
    cost: Amount,
}

#[derive(Serialize, Deserialize, Debug)]
struct GasCreditEvent {
    #[serde(flatten)]
    common: CommonEventFields,
    messageID: String,
    refundAddress: String,
    payment: Amount,
    meta: Option<Metadata>,
}

#[derive(Serialize, Deserialize, Debug)]
struct CannotExecuteMessageEvent {
    #[serde(flatten)]
    common: CommonEventFields,
    eventID: String,
    taskItemID: String,
    reason: String,
    details: Amount,
}

#[derive(Serialize, Deserialize, Debug)]
enum Event {
    Call(CallEvent),
    GasRefunded(GasRefundedEvent),
    GasCredit(GasCreditEvent),
    CannotExecuteMessage(CannotExecuteMessageEvent),
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct EventMessage {
    events: Vec<Event>,
}

impl GmpApi {
    pub fn new() -> Result<Self, GmpApiError> {
        let client = reqwest::ClientBuilder::new()
            .connect_timeout(DEFAULT_RPC_TIMEOUT.into())
            .timeout(DEFAULT_RPC_TIMEOUT)
            .build()
            .map_err(|e| GmpApiError::ConnectionFailed(e.to_string()))?;

        Ok(Self { client })
    }

    pub async fn get_tasks(&self) -> Result<(), GmpApiError> {
        let res = self
            .client
            .get(RPC_URL)
            .send()
            .await
            .map_err(|e| GmpApiError::RequestFailed(e.to_string()))?;

        match res.error_for_status_ref() {
            Ok(_) => {
                let response = res
                    .text()
                    .await
                    .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;

                let task_headers = serde_json::from_str::<CommonTaskFields>(&response)
                    .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;

                match task_headers.r#type.as_str() {
                    "EXECUTE" => {
                        let task = serde_json::from_str::<ExecuteTask>(&response)
                            .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;
                        info!("Received Execute task: {:?}", task);
                        return Ok(());
                    }
                    "REFUND" => {
                        let task = serde_json::from_str::<RefundTask>(&response)
                            .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;
                        info!("Received Refund task: {:?}", task);
                        return Ok(());
                    }
                    _ => {
                        warn!("Ignore unknown task.");
                        return Ok(());
                    }
                }
            }
            Err(e) => Err(GmpApiError::ErrorResponse(e.to_string())),
        }
    }

    pub async fn post_events(&self, events: Vec<Event>) -> Result<(), GmpApiError> {
        let msg: EventMessage = EventMessage { events };
        // self.client.post(RPC_URL).json();
        Ok(())
    }
}
