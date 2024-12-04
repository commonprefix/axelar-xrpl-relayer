use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Message {
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "sourceChain")]
    pub source_chain: String,
    #[serde(rename = "sourceAddress")]
    pub source_address: String,
    #[serde(rename = "destinationAddress")]
    pub destination_address: String,
    #[serde(rename = "payloadHash")]
    pub payload_hash: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Amount {
    #[serde(rename = "tokenID")]
    pub token_id: Option<String>,
    pub amount: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CommonTaskFields {
    pub id: String,
    pub timestamp: String,
    pub r#type: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ExecuteTaskFields {
    pub message: Message,
    pub payload: String,
    #[serde(rename = "availableGasBalance")]
    pub available_gas_balance: Amount,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ExecuteTask {
    #[serde(flatten)]
    pub common: CommonTaskFields,
    pub task: ExecuteTaskFields,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct RefundTaskFields {
    pub message: Message,
    #[serde(rename = "refundRecipientAddress")]
    pub refund_recipient_address: String,
    #[serde(rename = "remainingGasBalance")]
    pub remaining_gas_balance: Amount,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct RefundTask {
    #[serde(flatten)]
    pub common: CommonTaskFields,
    pub task: RefundTaskFields,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Task {
    Execute(ExecuteTask),
    Refund(RefundTask),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommonEventFields {
    pub r#type: String,
    #[serde(rename = "eventID")]
    pub event_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Metadata {
    #[serde(rename = "txID")]
    pub tx_id: String,
    #[serde(rename = "fromAddress")]
    pub from_address: String,
    pub finalized: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CallEvent {
    #[serde(flatten)]
    pub common: CommonEventFields,
    pub message: Message,
    #[serde(rename = "destinationChain")]
    pub destination_chain: String,
    pub payload: String,
    pub meta: Option<Metadata>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GasRefundedEvent {
    #[serde(flatten)]
    pub common: CommonEventFields,
    #[serde(rename = "recipientAddress")]
    pub recipient_address: String,
    #[serde(rename = "refundedAmount")]
    pub refunded_amount: Amount,
    pub cost: Amount,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GasCreditEvent {
    #[serde(flatten)]
    pub common: CommonEventFields,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "refundAddress")]
    pub refund_address: String,
    pub payment: Amount,
    pub meta: Option<Metadata>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CannotExecuteMessageEvent {
    #[serde(flatten)]
    pub common: CommonEventFields,
    #[serde(rename = "eventID")]
    pub event_id: String,
    #[serde(rename = "taskItemID")]
    pub task_item_id: String,
    pub reason: String,
    pub details: Amount,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Event {
    Call(CallEvent),
    GasRefunded(GasRefundedEvent),
    GasCredit(GasCreditEvent),
    CannotExecuteMessage(CannotExecuteMessageEvent),
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct PostEventResult {
    pub status: String,
    pub index: usize,
    pub error: Option<String>,
    pub retriable: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct PostEventResponse {
    pub results: Vec<PostEventResult>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct EventMessage {
    pub events: Vec<Event>,
}
