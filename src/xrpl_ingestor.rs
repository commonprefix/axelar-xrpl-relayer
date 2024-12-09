use core::str;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use xrpl_api::{PaymentTransaction, Transaction};

use crate::{
    error::IngestorError,
    gmp_api::GmpApi,
    gmp_types::{self, CommonEventFields, Event, Message, RouterMessage},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct XRPLUserMessage {
    // TODO: can this be imported?
    tx_id: String,
    source_address: String,
    destination_chain: String,
    destination_address: String,
    payload_hash: String,
    amount: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum QueryMsg {
    GetITSMessage(XRPLUserMessage), // TODO: can this be imported?
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

        let query = QueryMsg::GetITSMessage(xrpl_user_message);
        let its_hub_message: RouterMessage = serde_json::from_str(
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
                event_id: format!("{}-0", tx_hash), // TODO: should the log index be 0?
            },
            message: Message {
                message_id: its_hub_message.cc_id,
                source_chain: "xrpl".to_owned(),
                source_address: its_hub_message.source_address,
                destination_address: its_hub_message.destination_address,
                payload_hash: its_hub_message.payload_hash,
            },
            destination_chain: its_hub_message.destination_chain,
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
                event_id: "myeventid".to_owned(), // TODO: what should this be for gas credit events?
            },
            message_id: format!("{}-0", tx_hash), // TODO: should the log index be 0?
            refund_address: payment.common.account.clone(),
            payment: gmp_types::Amount {
                token_id: None, // TODO: should this be None when referring to Drops?
                amount: gas_amount.to_string(),
            },
            meta: None,
        })
    }
}
