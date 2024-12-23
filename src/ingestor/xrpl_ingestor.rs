use core::str;
use std::{collections::HashMap, sync::Arc, vec};

use multisig::key::PublicKey;
use router_api::CrossChainId;
use serde::{Deserialize, Serialize};
use tracing::debug;
use xrpl_amplifier_types::{
    msg::{XRPLMessage, XRPLUserMessage, XRPLUserMessageWithPayload},
    types::TxHash,
};
use xrpl_api::{Memo, PaymentTransaction, Transaction, TxRequest};
use xrpl_gateway::msg::InterchainTransfer;

use crate::{
    config::Config,
    error::IngestorError,
    gmp_api::{
        gmp_types::{
            self, Amount, BroadcastRequest, CommonEventFields, ConstructProofTask, Event, Metadata,
            QueryRequest, ReactToWasmEventTask, VerifyTask,
        },
        GmpApi,
    },
    payload_cache::PayloadCacheClient,
    utils::extract_from_xrpl_memo,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum QueryMsg {
    GetITSMessage(XRPLUserMessage), // TODO: can this be imported?
}

fn extract_memo(memos: &Option<Vec<Memo>>, memo_type: &str) -> Result<String, IngestorError> {
    extract_from_xrpl_memo(memos.clone(), memo_type).map_err(|e| {
        IngestorError::GenericError(format!(
            "Failed to extract {} from memos: {}",
            memo_type,
            e.to_string()
        ))
    })
}

async fn build_xrpl_user_message(
    payload_cache: &PayloadCacheClient,
    payment: &PaymentTransaction,
) -> Result<XRPLUserMessageWithPayload, IngestorError> {
    let tx_hash = payment.common.hash.clone().ok_or_else(|| {
        IngestorError::GenericError("Payment transaction missing field 'hash'".to_owned())
    })?;

    let memos = &payment.common.memos;

    let destination_address = extract_memo(memos, "destination_address")?;
    let destination_chain = extract_memo(memos, "destination_chain")?;
    // let deposit_amount = extract_memo(memos, "deposit")?;
    let deposit_amount = payment.amount.size().to_string(); // TODO: get from memo
    let payload_hash_memo = extract_memo(memos, "payload_hash");
    let payload_memo = extract_memo(memos, "payload");
    let payload;
    let payload_hash;

    if payload_memo.is_ok() && payload_hash_memo.is_ok() {
        return Err(IngestorError::GenericError(
            "Payment transaction cannot have both 'payload' and 'payload_hash' memos".to_owned(),
        ));
    } else if payload_memo.is_ok() {
        payload = Some(payload_memo.unwrap());
        payload_hash = payload_cache
            .store_payload(&payload.clone().unwrap())
            .await
            .map_err(|e| {
                IngestorError::GenericError(format!(
                    "Failed to store payload in cache: {}",
                    e.to_string()
                ))
            })?;
    } else if payload_hash_memo.is_ok() {
        payload_hash = payload_hash_memo.unwrap();
        if payload_hash == "0".repeat(64) {
            payload = None;
        } else {
            payload = Some(
                payload_cache
                    .get_payload(&hex::encode(payload_hash.clone()))
                    .await
                    .map_err(|e| {
                        IngestorError::GenericError(format!(
                            "Failed to get payload from cache: {}",
                            e.to_string()
                        ))
                    })?,
            );
        }
    } else {
        payload = None;
        payload_hash = "0".repeat(64);
    }

    let mut message_with_payload = XRPLUserMessageWithPayload {
        message: XRPLUserMessage {
            tx_id: hex::decode(tx_hash.clone())
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap(),
            source_address: payment.common.account.clone().try_into().unwrap(),
            destination_address: hex::decode(destination_address)
                .unwrap()
                .try_into()
                .unwrap(),
            destination_chain: str::from_utf8(hex::decode(destination_chain).unwrap().as_slice())
                .unwrap()
                .try_into()
                .unwrap(),
            payload_hash: hex::decode(payload_hash).unwrap().try_into().unwrap(),
            amount: xrpl_amplifier_types::types::XRPLPaymentAmount::Drops(
                payment.amount.size() as u64
            ), // TODO: Recover this
               // amount: xrpl_amplifier_types::types::XRPLPaymentAmount::Drops(
               //     str::from_utf8(hex::decode(deposit_amount).unwrap().as_slice())
               //         .unwrap()
               //         .parse::<u64>()
               //         .unwrap(),
               // ),
        },
        payload: None,
    };

    if payload.is_some() {
        message_with_payload.payload =
            Some(payload.unwrap().as_bytes().to_vec().try_into().unwrap());
    }

    Ok(message_with_payload)
}

pub struct XrplIngestor {
    client: xrpl_http_client::Client,
    gmp_api: Arc<GmpApi>,
    config: Config,
    payload_cache: PayloadCacheClient,
}

fn parse_message_from_context(metadata: Option<Metadata>) -> Result<XRPLMessage, IngestorError> {
    let source_context = metadata
        .ok_or(IngestorError::GenericError(
            "Verify task missing meta field".to_owned(),
        ))?
        .source_context
        .ok_or(IngestorError::GenericError(
            "Verify task missing source_context field".to_owned(),
        ))?;

    Ok(source_context
        .get(&"user_message".to_owned())
        .ok_or(IngestorError::GenericError(
            "Verify task missing user_message in source_context".to_owned(),
        ))
        .map_err(|e| {
            IngestorError::GenericError(format!(
                "Failed to parse source context to XRPL User Message: {}",
                e.to_string()
            ))
        })?
        .to_owned())
}

impl XrplIngestor {
    pub fn new(gmp_api: Arc<GmpApi>, config: Config) -> Self {
        let client = xrpl_http_client::Client::builder()
            .base_url(&config.xrpl_rpc)
            .build();
        let payload_cache = PayloadCacheClient::new(&config.payload_cache);
        Self {
            gmp_api,
            config,
            client,
            payload_cache,
        }
    }

    pub async fn handle_transaction(&self, tx: Transaction) -> Result<Vec<Event>, IngestorError> {
        match tx {
            Transaction::Payment(payment) => {
                if payment.destination == self.config.multisig_address {
                    self.handle_payment(payment).await
                } else if payment.common.account == self.config.multisig_address {
                    // prover message
                    self.handle_prover_tx(payment).await
                } else {
                    Err(IngestorError::UnsupportedTransaction(
                        serde_json::to_string(&payment).unwrap(),
                    ))
                }
            }
            Transaction::TrustSet(_) => {
                Err(IngestorError::UnsupportedTransaction("TrustSet".to_owned()))
            }
            Transaction::SignerListSet(_) => Err(IngestorError::UnsupportedTransaction(
                "SignerListSet".to_owned(),
            )),
            Transaction::TicketCreate(_) => Err(IngestorError::UnsupportedTransaction(
                "TicketCreate".to_owned(),
            )),
            tx => Err(IngestorError::UnsupportedTransaction(
                serde_json::to_string(&tx).unwrap(),
            )),
        }
    }

    pub async fn handle_payment(
        &self,
        payment: PaymentTransaction,
    ) -> Result<Vec<Event>, IngestorError> {
        let call_event = self.call_event_from_payment(&payment).await?;
        let gas_credit_event = self.gas_credit_event_from_payment(&payment).await?;

        return Ok(vec![call_event, gas_credit_event]);
    }

    pub async fn handle_prover_tx(
        &self,
        payment: PaymentTransaction,
    ) -> Result<Vec<Event>, IngestorError> {
        let execute_msg =
            xrpl_gateway::msg::ExecuteMsg::VerifyMessages(vec![XRPLMessage::ProverMessage(
                hex::decode(payment.common.hash.unwrap())
                    .unwrap()
                    .as_slice()
                    .try_into()
                    .unwrap(),
            )]);
        let request = BroadcastRequest::Generic(serde_json::to_value(&execute_msg).unwrap());
        self.gmp_api
            .post_broadcast(self.config.xrpl_gateway_address.clone(), &request)
            .await
            .map_err(|e| {
                IngestorError::GenericError(format!(
                    "Failed to broadcast message: {}",
                    e.to_string()
                ))
            })?;

        Ok(vec![])
    }

    async fn call_event_from_payment(
        &self,
        payment: &PaymentTransaction,
    ) -> Result<Event, IngestorError> {
        let xrpl_user_message_with_payload =
            build_xrpl_user_message(&self.payload_cache, payment).await?;
        let xrpl_user_message = xrpl_user_message_with_payload.message.clone();

        let source_context = HashMap::from([(
            "user_message".to_owned(),
            XRPLMessage::UserMessage(xrpl_user_message.clone()),
        )]);

        let query = xrpl_gateway::msg::QueryMsg::InterchainTransfer {
            message_with_payload: xrpl_user_message_with_payload.clone(),
        };
        let request = QueryRequest::Generic(serde_json::to_value(&query).unwrap());
        let interchain_transfer_response: InterchainTransfer = serde_json::from_str(
            &self
                .gmp_api
                .post_query(self.config.xrpl_gateway_address.clone(), &request)
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
                event_id: xrpl_user_message.tx_id.to_string(),
            },
            message: interchain_transfer_response
                .message_with_payload
                .unwrap()
                .message,
            destination_chain: xrpl_user_message.destination_chain.to_string(),
            payload: xrpl_user_message_with_payload
                .payload
                .map(|p| p.to_hex())
                .unwrap_or_else(|| "".to_string()),
            meta: Some(Metadata {
                tx_id: None,
                from_address: None,
                finalized: None,
                source_context: Some(source_context),
            }),
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
        let deposit_amount = total_amount; // TODO: get from memo
                                           // let deposit_amount_memo = extract_memo(&payment.common.memos, "deposit")?;
                                           // let deposit_amount = str::from_utf8(hex::decode(deposit_amount_memo).unwrap().as_slice())
                                           //     .unwrap()
                                           //     .to_string()
                                           //     .parse::<u64>()
                                           //     .unwrap(); // TODO: should this be a u64?
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
        let xrpl_message = parse_message_from_context(task.common.meta)?;
        let user_message = match xrpl_message {
            XRPLMessage::UserMessage(user_message) => user_message,
            _ => {
                return Err(IngestorError::GenericError(
                    "Verify task message is not a UserMessage".to_owned(),
                ))
            }
        };

        let execute_msg =
            xrpl_gateway::msg::ExecuteMsg::VerifyMessages(vec![XRPLMessage::UserMessage(
                user_message.clone(),
            )]);
        let request = BroadcastRequest::Generic(serde_json::to_value(&execute_msg).unwrap());
        Ok(self
            .gmp_api
            .post_broadcast(self.config.xrpl_gateway_address.clone(), &request)
            .await
            .map_err(|e| {
                IngestorError::GenericError(format!(
                    "Failed to broadcast message: {}",
                    e.to_string()
                ))
            })?)
    }

    pub async fn prover_tx_routing_request(
        &self,
        tx_hash: TxHash,
    ) -> Result<(String, BroadcastRequest), IngestorError> {
        let tx_request = TxRequest::new(&tx_hash.to_string()).binary(false);
        let res = self.client.call(tx_request).await.map_err(|e| {
            IngestorError::GenericError(format!("Failed to get transaction: {}", e.to_string()))
        })?;
        match res.tx {
            Transaction::Payment(payment_transaction) => {
                let multisig_session_id_hex =
                    extract_from_xrpl_memo(payment_transaction.common.memos, "multisig_session_id")
                        .map_err(|e| {
                            IngestorError::GenericError(format!(
                                "Failed to extract multisig_session_id from memos: {}",
                                e.to_string()
                            ))
                        })?;
                let multisig_session_id = u64::from_str_radix(&multisig_session_id_hex, 16)
                    .map_err(|e| {
                        IngestorError::GenericError(format!(
                            "Failed to parse multisig_session_id: {}",
                            e.to_string()
                        ))
                    })?;

                let signers = payment_transaction.common.signers.clone().ok_or(
                    IngestorError::GenericError("Payment transaction missing signers".to_owned()),
                )?;
                let signers_keys = signers
                    .iter()
                    .map(|signer| {
                        serde_json::from_str::<PublicKey>(&format!(
                            "{{ \"ecdsa\": \"{}\"}}", // TODO: beautify
                            signer.signer.signing_pub_key
                        ))
                        .unwrap()
                    })
                    .collect::<Vec<PublicKey>>();

                let execute_msg = xrpl_multisig_prover::msg::ExecuteMsg::ConfirmTxStatus {
                    signer_public_keys: signers_keys,
                    signed_tx_hash: TxHash::new(
                        hex::decode(payment_transaction.common.hash.unwrap())
                            .unwrap()
                            .try_into()
                            .unwrap(),
                    ),
                    multisig_session_id: multisig_session_id.try_into().unwrap(),
                };
                let request =
                    BroadcastRequest::Generic(serde_json::to_value(&execute_msg).unwrap());
                Ok((self.config.multisig_address.clone(), request))
            }
            Transaction::SignerListSet(_) => todo!(),
            Transaction::TicketCreate(_) => todo!(),
            Transaction::TrustSet(_) => todo!(),
            _ => Err(IngestorError::UnsupportedTransaction(
                "Unsupported transaction type".to_owned(),
            )),
        }
    }

    pub fn user_message_routing_request(
        &self,
        user_message: XRPLUserMessage,
    ) -> Result<(String, BroadcastRequest), IngestorError> {
        let execute_msg = xrpl_gateway::msg::ExecuteMsg::RouteIncomingMessages(vec![
            XRPLUserMessageWithPayload {
                message: user_message,
                payload: None, // TODO
            },
        ]);
        let request = BroadcastRequest::Generic(serde_json::to_value(&execute_msg).unwrap());
        Ok((self.config.xrpl_gateway_address.clone(), request))
    }

    pub async fn handle_wasm_event(&self, task: ReactToWasmEventTask) -> Result<(), IngestorError> {
        let event_name = task.task.event_name.clone();

        match task.task.event_name.as_str() {
            "wasm-quorum-reached" => {
                let xrpl_message = task.task.message.clone();
                let (contract_address, request) = match xrpl_message.clone() {
                    XRPLMessage::UserMessage(user_message) => {
                        debug!("Quorum reached for XRPLUserMessage: {:?}", user_message);
                        self.user_message_routing_request(user_message)?
                    }
                    XRPLMessage::ProverMessage(tx_hash) => {
                        debug!("Quorum reached for XRPLProverMessage: {:?}", tx_hash);
                        self.prover_tx_routing_request(tx_hash).await?
                    }
                };
                debug!("Broadcasting request: {:?}", request);

                // TODO: think what happens on failure. This shouldn't happen.
                self.gmp_api
                    .post_broadcast(contract_address, &request)
                    .await
                    .map_err(|e| {
                        IngestorError::GenericError(format!(
                            "Failed to broadcast message: {}",
                            e.to_string()
                        ))
                    })?;

                match xrpl_message {
                    XRPLMessage::ProverMessage(_) => {
                        // TODO: fill in fields
                        let event = Event::MessageExecuted {
                            common: CommonEventFields {
                                r#type: "MESSAGE_EXECUTED".to_owned(),
                                event_id: "TODO".to_owned(),
                            },
                            message_id: "id".to_owned(),
                            source_chain: "source".to_owned(),
                            cost: Amount {
                                token_id: None,
                                amount: "0".to_owned(),
                            },
                            meta: None,
                        };
                        let events_response =
                            self.gmp_api.post_events(vec![event]).await.map_err(|e| {
                                IngestorError::GenericError(format!(
                                    "Failed to broadcast message: {}",
                                    e.to_string()
                                ))
                            })?;
                        let response =
                            events_response.get(0).ok_or(IngestorError::GenericError(
                                "Failed to get response from posting events".to_owned(),
                            ))?;
                        if response.status != "ACCEPTED" {
                            return Err(IngestorError::GenericError(format!(
                                "Failed to post event: {}",
                                response.error.clone().unwrap_or_default()
                            )));
                        }
                    }
                    _ => {}
                };
                Ok(())
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
        let execute_msg = xrpl_multisig_prover::msg::ExecuteMsg::ConstructProof {
            cc_id: CrossChainId::new(task.task.message.source_chain, task.task.message.message_id)
                .unwrap(),
            payload: hex::decode(task.task.payload).unwrap().into(),
        };

        let request = BroadcastRequest::Generic(serde_json::to_value(&execute_msg).unwrap());
        Ok(self
            .gmp_api
            .post_broadcast(self.config.xrpl_multisig_prover_address.clone(), &request)
            .await
            .map_err(|e| {
                IngestorError::GenericError(format!(
                    "Failed to broadcast message: {}",
                    e.to_string()
                ))
            })?)
    }
}
