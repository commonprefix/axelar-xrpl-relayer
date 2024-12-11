use core::str;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use xrpl_api::{PaymentTransaction, Transaction};

use crate::{
    error::IngestorError,
    gmp_api::GmpApi,
    gmp_types::{
        self, BroadcastRequest, CommonEventFields, ConstructProofTask, Event, GatewayV2Message,
        ReactToWasmEventTask, VerifyTask,
    },
};

#[derive(Debug, Serialize, Deserialize)]
pub enum XRPLMessage {
    // TODO: import
    ProverMessage(String),
    UserMessage(XRPLUserMessage),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct XRPLUserMessage {
    // TODO: can this be imported?
    tx_id: String,
    source_address: String,
    destination_chain: String,
    destination_address: String,
    payload_hash: String,
    amount: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct XRPLUserMessageWithPayload {
    message: XRPLUserMessage,
    payload: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum QueryMsg {
    GetITSMessage(XRPLUserMessage), // TODO: can this be imported?
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecuteMessage {
    VerifyMessages(Vec<XRPLUserMessage>), // TODO: can this be imported?
    RouteIncomingMessages(XRPLUserMessageWithPayload), // TODO: can this be imported?
    ConstructProof { cc_id: String, payload: String },
}

pub struct XrplIngestor {
    gmp_api: Arc<GmpApi>,
    multisig_address: String,
}

impl XrplIngestor {
    pub fn new(gmp_api: Arc<GmpApi>, multisig_address: String) -> Self {
        Self {
            gmp_api,
            multisig_address,
        }
    }

    pub async fn handle_transaction(&self, tx: Transaction) -> Result<Vec<Event>, IngestorError> {
        match tx {
            Transaction::Payment(payment) => self.handle_payment(payment).await,
            Transaction::TrustSet(_) => {
                Err(IngestorError::UnsupportedTransaction("TrustSet".to_owned()))
            }
            Transaction::SignerListSet(_) => {
                Err(IngestorError::UnsupportedTransaction("TrustSet".to_owned()))
            }
            Transaction::TicketCreate(_) => {
                Err(IngestorError::UnsupportedTransaction("TrustSet".to_owned()))
            }
            tx => Err(IngestorError::UnsupportedTransaction(
                serde_json::to_string(&tx).unwrap(),
            )),
        }
    }

    pub async fn handle_payment(
        &self,
        payment: PaymentTransaction,
    ) -> Result<Vec<Event>, IngestorError> {
        if payment.destination != self.multisig_address {
            return Ok(Vec::new());
        }

        let call_event = self.call_event_from_payment(&payment).await?;
        let gas_credit_event = self.gas_credit_event_from_payment(&payment).await?;

        return Ok(vec![call_event, gas_credit_event]);
    }

    async fn call_event_from_payment(
        &self,
        payment: &PaymentTransaction,
    ) -> Result<Event, IngestorError> {
        let tx_hash = payment
            .common
            .hash
            .clone()
            .ok_or(IngestorError::GenericError(
                "Payment transaction missing field 'hash'".to_owned(),
            ))?;
        let xrpl_user_message = XRPLUserMessage {
            tx_id: tx_hash.clone(),
            source_address: payment.common.account.clone(),
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
            payload_hash: payment.common.memos.clone().unwrap()[2]
                .memo_data
                .clone()
                .unwrap(),
            amount: payment.common.memos.clone().unwrap()[3]
                .memo_data
                .clone()
                .unwrap(), // TODO: Assumption: the actual amount to be wrapped is in the third memo as a string (drops)
        };

        let query = QueryMsg::GetITSMessage(xrpl_user_message.clone());
        let its_hub_message: GatewayV2Message = serde_json::from_str(
            &self
                .gmp_api
                .post_query(
                    "".to_owned(),
                    serde_json::to_string(&query).unwrap().as_bytes(),
                )
                .await
                .map_err(|e| {
                    IngestorError::GenericError(format!(
                        "Failed to translate XRPL User Message to ITS Message: {}",
                        e.to_string()
                    ))
                })?,
        )
        .map_err(|e| {
            IngestorError::GenericError(format!("Failed to parse ITS Message: {}", e.to_string()))
        })?;

        Ok(Event::Call {
            common: CommonEventFields {
                r#type: "CALL".to_owned(),
                event_id: tx_hash.clone(),
            },
            message: its_hub_message,
            destination_chain: xrpl_user_message.destination_chain,
            payload: "".to_owned(), // TODO
            meta: None,
        })
    }

    async fn gas_credit_event_from_payment(
        &self,
        payment: &PaymentTransaction,
    ) -> Result<Event, IngestorError> {
        let tx_hash = payment
            .common
            .hash
            .clone()
            .ok_or(IngestorError::GenericError(
                "Payment transaction missing field 'hash'".to_owned(),
            ))?;
        let total_amount = payment.amount.size() as u64; // TODO: size should probably not be used
        let deposit_amount = payment.common.memos.clone().unwrap()[3]
            .memo_data
            .clone()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let gas_amount = total_amount - deposit_amount;
        Ok(Event::GasCredit {
            common: CommonEventFields {
                r#type: "GAS_CREDIT".to_owned(),
                event_id: tx_hash.clone(),
            },
            message_id: tx_hash, // TODO: Should this be the its hub message id?
            refund_address: payment.common.account.clone(),
            payment: gmp_types::Amount {
                token_id: None, // TODO: should this be None when referring to Drops?
                amount: gas_amount.to_string(),
            },
            meta: None,
        })
    }

    pub async fn handle_verify(&self, task: VerifyTask) -> Result<(), IngestorError> {
        let source_context = task
            .common
            .meta
            .ok_or(IngestorError::GenericError(
                "Verify task missing meta field".to_owned(),
            ))?
            .source_context
            .ok_or(IngestorError::GenericError(
                "Verify task missing source_context field".to_owned(),
            ))?;

        let user_message: &XRPLUserMessage = source_context
            .get(&"user_message".to_owned())
            .ok_or(IngestorError::GenericError(
                "Verify task missing user_message in source_context".to_owned(),
            ))
            .map_err(|e| {
                IngestorError::GenericError(format!(
                    "Failed to parse source context to XRPL User Message: {}",
                    e.to_string()
                ))
            })?;

        let message = ExecuteMessage::VerifyMessages(vec![user_message.clone()]);
        let request = BroadcastRequest::Generic(serde_json::to_value(&message).unwrap());
        Ok(self
            .gmp_api
            .post_broadcast("contract".to_owned(), &request)
            .await
            .map_err(|e| {
                IngestorError::GenericError(format!(
                    "Failed to broadcast message: {}",
                    e.to_string()
                ))
            })?)
    }

    pub async fn handle_wasm_event(&self, task: ReactToWasmEventTask) -> Result<(), IngestorError> {
        let event_name = task.task.event_name.clone();

        match task.task.event_name.as_str() {
            "wasm-quorum-reached" => {
                // TODO: handle prover messages
                let user_message = task.task.message.clone();

                let message = ExecuteMessage::RouteIncomingMessages(XRPLUserMessageWithPayload {
                    message: user_message,
                    payload: None, // TODO
                });
                let request = BroadcastRequest::Generic(serde_json::to_value(&message).unwrap());
                Ok(self
                    .gmp_api
                    .post_broadcast("".to_owned(), &request)
                    .await
                    .map_err(|e| {
                        IngestorError::GenericError(format!(
                            "Failed to broadcast message: {}",
                            e.to_string()
                        ))
                    })?)
            }
            _ => Err(IngestorError::GenericError(format!(
                "Unknown event name: {}",
                event_name
            ))),
        }
    }

    pub async fn handle_construct_proof(
        &self,
        task: ConstructProofTask,
    ) -> Result<(), IngestorError> {
        let message = ExecuteMessage::ConstructProof {
            cc_id: format!(
                "{}:{}",
                task.task.message.source_chain, task.task.message.message_id
            ), // TODO: Import CrossChainID from amplifier
            payload: task.task.payload,
        };

        let request = BroadcastRequest::Generic(serde_json::to_value(&message).unwrap());
        Ok(self
            .gmp_api
            .post_broadcast("contract".to_owned(), &request)
            .await
            .map_err(|e| {
                IngestorError::GenericError(format!(
                    "Failed to broadcast message: {}",
                    e.to_string()
                ))
            })?)
    }
}
