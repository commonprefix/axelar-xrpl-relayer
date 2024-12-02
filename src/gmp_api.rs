use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, time::Duration};
use tracing::{info, warn};

use reqwest::Client;

use crate::{error::GmpApiError, utils::parse_task};

pub struct GmpApi {
    rpc_url: String,
    client: Client,
}

const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(3);
const RPC_URL: &str = "https://s.devnet.rippletest.net:51234";

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Message {
    #[serde(rename = "messageID")]
    message_id: String,
    #[serde(rename = "sourceChain")]
    source_chain: String,
    #[serde(rename = "sourceAddress")]
    source_address: String,
    #[serde(rename = "destinationAddress")]
    destination_address: String,
    #[serde(rename = "payloadHash")]
    payload_hash: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Amount {
    #[serde(rename = "tokenID")]
    token_id: Option<String>,
    amount: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CommonTaskFields {
    pub id: String,
    pub timestamp: String,
    pub r#type: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ExecuteTaskFields {
    message: Message,
    payload: String,
    #[serde(rename = "availableGasBalance")]
    available_gas_balance: Amount,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ExecuteTask {
    #[serde(flatten)]
    common: CommonTaskFields,
    task: ExecuteTaskFields,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct RefundTaskFields {
    message: Message,
    #[serde(rename = "refundRecipientAddress")]
    refund_recipient_address: String,
    #[serde(rename = "remainingGasBalance")]
    remaining_gas_balance: Amount,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct RefundTask {
    #[serde(flatten)]
    common: CommonTaskFields,
    task: RefundTaskFields,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Task {
    Execute(ExecuteTask),
    Refund(RefundTask),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommonEventFields {
    r#type: String,
    #[serde(rename = "eventID")]
    event_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Metadata {
    #[serde(rename = "txID")]
    tx_id: String,
    #[serde(rename = "fromAddress")]
    from_address: String,
    finalized: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CallEvent {
    #[serde(flatten)]
    common: CommonEventFields,
    message: Message,
    #[serde(rename = "destinationChain")]
    destination_chain: String,
    payload: String,
    meta: Option<Metadata>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GasRefundedEvent {
    #[serde(flatten)]
    common: CommonEventFields,
    #[serde(rename = "recipientAddress")]
    recipient_address: String,
    #[serde(rename = "refundedAmount")]
    refunded_amount: Amount,
    cost: Amount,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GasCreditEvent {
    #[serde(flatten)]
    common: CommonEventFields,
    #[serde(rename = "messageID")]
    message_id: String,
    #[serde(rename = "refundAddress")]
    refund_address: String,
    payment: Amount,
    meta: Option<Metadata>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CannotExecuteMessageEvent {
    #[serde(flatten)]
    common: CommonEventFields,
    #[serde(rename = "eventID")]
    event_id: String,
    #[serde(rename = "taskItemID")]
    task_item_id: String,
    reason: String,
    details: Amount,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Event {
    Call(CallEvent),
    GasRefunded(GasRefundedEvent),
    GasCredit(GasCreditEvent),
    CannotExecuteMessage(CannotExecuteMessageEvent),
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct PostEventResult {
    status: String,
    index: usize,
    error: Option<String>,
    retriable: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct PostEventResponse {
    results: Vec<PostEventResult>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct EventMessage {
    events: Vec<Event>,
}

impl GmpApi {
    pub fn new(rpc_url: &str) -> Result<Self, GmpApiError> {
        let client = reqwest::ClientBuilder::new()
            .connect_timeout(DEFAULT_RPC_TIMEOUT.into())
            .timeout(DEFAULT_RPC_TIMEOUT)
            .build()
            .map_err(|e| GmpApiError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            rpc_url: rpc_url.to_owned(),
            client,
        })
    }

    pub async fn get_tasks(&self) -> Result<Vec<Task>, GmpApiError> {
        let res = self
            .client
            .get(&format!("{}/tasks", self.rpc_url))
            .send()
            .await
            .map_err(|e| GmpApiError::RequestFailed(e.to_string()))?;

        res.error_for_status_ref()
            .map_err(|e| GmpApiError::ErrorResponse(e.to_string()))?;

        let response: HashMap<String, Vec<Value>> = res
            .json()
            .await
            .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;

        let tasks_json = response
            .get("tasks")
            .ok_or_else(|| GmpApiError::InvalidResponse("Missing 'tasks' field".to_string()))?;

        Ok(tasks_json
            .iter()
            .filter_map(|task_json| match parse_task(task_json) {
                Ok(task) => Some(task),
                Err(e) => {
                    warn!("Failed to parse task: {:?}", e);
                    None
                }
            })
            .collect::<Vec<_>>())
    }

    pub async fn post_events(
        &self,
        events: Vec<Event>,
    ) -> Result<Vec<PostEventResult>, GmpApiError> {
        let mut map = HashMap::new();
        map.insert("events", events);

        let res = self
            .client
            .post(&format!("{}/events", self.rpc_url))
            .json(&map)
            .send()
            .await
            .map_err(|e| GmpApiError::RequestFailed(e.to_string()))?;

        match res.error_for_status_ref() {
            Ok(_) => {
                let response: PostEventResponse = res
                    .json()
                    .await
                    .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;
                info!("Response from POST: {:?}", response);
                Ok(response.results)
            }
            Err(e) => Err(GmpApiError::ErrorResponse(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_get_tasks() {
        let response = r#"
        {
            "tasks": [
                {
                    "id": "task1",
                    "timestamp": "2024-01-01T12:00:00Z",
                    "type": "EXECUTE",
                    "task": {
                        "message": {
                            "messageID": "msg1",
                            "sourceChain": "chainA",
                            "sourceAddress": "srcAddr",
                            "destinationAddress": "destAddr",
                            "payloadHash": "payloadHash123"
                        },
                        "payload": "payloadData",
                        "availableGasBalance": {
                            "tokenID": null,
                            "amount": "1000"
                        }
                    }
                },
                {
                    "id": "task2",
                    "timestamp": "2024-01-01T12:30:00Z",
                    "type": "REFUND",
                    "task": {
                        "message": {
                            "messageID": "msg2",
                            "sourceChain": "chainB",
                            "sourceAddress": "srcAddrB",
                            "destinationAddress": "destAddrB",
                            "payloadHash": "payloadHash456"
                        },
                        "refundRecipientAddress": "refundAddr",
                        "remainingGasBalance": {
                            "tokenID": "token123",
                            "amount": "2000"
                        }
                    }
                }
            ]
        }
        "#;

        let mut server = mockito::Server::new_async().await;
        let url = server.url();

        let _mock = server
            .mock("GET", "/tasks")
            .with_status(200)
            .with_body(response)
            .create();

        let api = GmpApi::new(&url).unwrap();

        let result = api.get_tasks().await;

        assert!(result.is_ok());
        let tasks = result.unwrap();

        assert_eq!(
            tasks[0],
            Task::Execute(ExecuteTask {
                common: CommonTaskFields {
                    id: "task1".to_string(),
                    timestamp: "2024-01-01T12:00:00Z".to_string(),
                    r#type: "EXECUTE".to_string(),
                },
                task: ExecuteTaskFields {
                    message: Message {
                        message_id: "msg1".to_string(),
                        source_chain: "chainA".to_string(),
                        source_address: "srcAddr".to_string(),
                        destination_address: "destAddr".to_string(),
                        payload_hash: "payloadHash123".to_string()
                    },
                    payload: "payloadData".to_string(),
                    available_gas_balance: Amount {
                        token_id: None,
                        amount: "1000".to_string()
                    }
                }
            })
        );

        assert_eq!(
            tasks[1],
            Task::Refund(RefundTask {
                common: CommonTaskFields {
                    id: "task2".to_string(),
                    timestamp: "2024-01-01T12:30:00Z".to_string(),
                    r#type: "REFUND".to_string(),
                },
                task: RefundTaskFields {
                    message: Message {
                        message_id: "msg2".to_string(),
                        source_chain: "chainB".to_string(),
                        source_address: "srcAddrB".to_string(),
                        destination_address: "destAddrB".to_string(),
                        payload_hash: "payloadHash456".to_string()
                    },
                    refund_recipient_address: "refundAddr".to_string(),
                    remaining_gas_balance: Amount {
                        token_id: Some("token123".to_string()),
                        amount: "2000".to_string()
                    }
                }
            })
        );
    }

    #[tokio::test]
    async fn test_post_events_response_parsing() {
        let response = r#"
    {
        "results": [
            {
                "status": "ACCEPTED",
                "index": 0
            },
            {
                "status": "REJECTED",
                "index": 1,
                "error": "Invalid event data",
                "retriable": true
            }
        ]
    }
    "#;

        let mut server = mockito::Server::new_async().await;
        let url = server.url();

        let _mock = server
            .mock("POST", "/events")
            .with_status(200)
            .with_body(response)
            .create();

        let api = GmpApi::new(&url).unwrap();

        let events = vec![];
        let result = api.post_events(events).await;

        match result {
            Ok(results) => {
                assert_eq!(results.len(), 2);
                assert_eq!(results[0].status, "ACCEPTED");
                assert_eq!(results[0].index, 0);
                assert_eq!(results[0].error, None);
                assert_eq!(results[0].retriable, None);

                assert_eq!(results[1].status, "REJECTED");
                assert_eq!(results[1].index, 1);
                assert_eq!(results[1].error.as_deref(), Some("Invalid event data"));
                assert_eq!(results[1].retriable, Some(true));
            }
            Err(e) => panic!("Failed to post events: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_post_events_verify_request_body() {
        let mut server = mockito::Server::new_async().await;
        let url = server.url();

        let response_body = r#"
            {
                "results": [
                    {
                        "status": "ACCEPTED",
                        "index": 0
                    }
                ]
            }
        "#;

        let _mock = server
            .mock("POST", "/events")
            .match_body(r#"{"events":[{"type":"CALL","eventID":"event1","message":{"messageID":"msg1","sourceChain":"chainA","sourceAddress":"srcAddrA","destinationAddress":"destAddrA","payloadHash":"payload123"},"destinationChain":"chainB","payload":"payloadData","meta":null}]}"#)
            .with_status(200)
            .with_body(response_body)
            .create_async()
            .await;

        let api = GmpApi::new(&url).unwrap();

        let events = vec![Event::Call(CallEvent {
            common: CommonEventFields {
                r#type: "CALL".to_string(),
                event_id: "event1".to_string(),
            },
            message: Message {
                message_id: "msg1".to_string(),
                source_chain: "chainA".to_string(),
                source_address: "srcAddrA".to_string(),
                destination_address: "destAddrA".to_string(),
                payload_hash: "payload123".to_string(),
            },
            destination_chain: "chainB".to_string(),
            payload: "payloadData".to_string(),
            meta: None,
        })];

        let result = api.post_events(events).await;

        assert!(result.is_ok());
    }
}
