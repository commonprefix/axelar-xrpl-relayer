use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::{info, warn};
use xrpl_api::Memo;

use crate::{
    error::GmpApiError,
    gmp_api::gmp_types::{
        CommonTaskFields, ConstructProofTask, ExecuteTask, GatewayTxTask, ReactToWasmEventTask,
        RefundTask, Task, VerifyTask,
    },
};

fn parse_as<T: DeserializeOwned>(value: &Value) -> Result<T, GmpApiError> {
    serde_json::from_value(value.clone()).map_err(|e| GmpApiError::InvalidResponse(e.to_string()))
}

pub fn parse_task(task_json: &Value) -> Result<Task, GmpApiError> {
    let task_headers: CommonTaskFields = serde_json::from_value(task_json.clone())
        .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;

    // TODO: DRY
    match task_headers.r#type.as_str() {
        "CONSTRUCT_PROOF" => {
            let task: ConstructProofTask = parse_as(task_json)?;
            info!("Parsed ConstructProof task: {:?}", task);
            Ok(Task::ConstructProof(task))
        }
        "GATEWAY_TX" => {
            let task: GatewayTxTask = parse_as(task_json)?;
            info!("Parsed GatewayTx task: {:?}", task);
            Ok(Task::GatewayTx(task))
        }
        "VERIFY" => {
            let task: VerifyTask = parse_as(task_json)?;
            info!("Parsed VerifyTask task: {:?}", task);
            Ok(Task::Verify(task))
        }
        "EXECUTE" => {
            let task: ExecuteTask = parse_as(task_json)?;
            info!("Parsed Execute task: {:?}", task);
            Ok(Task::Execute(task))
        }
        "REFUND" => {
            let task: RefundTask = parse_as(task_json)?;
            info!("Parsed Refund task: {:?}", task);
            Ok(Task::Refund(task))
        }
        "REACT_TO_WASM_EVENT" => {
            let task: ReactToWasmEventTask = parse_as(task_json)?;
            info!("Parsed ReactToWasmEvent task: {:?}", task);
            Ok(Task::ReactToWasmEvent(task))
        }
        _ => {
            warn!("Unknown task type: {:?}", task_headers.r#type);
            Err(GmpApiError::InvalidResponse(
                "Unknown task type".to_string(),
            ))
        }
    }
}

pub fn extract_from_xrpl_memo(
    memos: Option<Vec<Memo>>,
    memo_type: &str,
) -> Result<String, anyhow::Error> {
    let memos = memos.ok_or_else(|| anyhow::anyhow!("No memos"))?;
    let desired_type_hex = hex::encode(memo_type).to_lowercase();

    if let Some(memo) = memos.into_iter().find(|m| {
        m.memo_type
            .as_ref()
            .map(|t| t.to_lowercase())
            .unwrap_or_default()
            == desired_type_hex
    }) {
        Ok(memo
            .memo_data
            .ok_or_else(|| anyhow::anyhow!("memo_data is missing"))?)
    } else {
        Err(anyhow::anyhow!("No memo with type: {}", memo_type))
    }
}
