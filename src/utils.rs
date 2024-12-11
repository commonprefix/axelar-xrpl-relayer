use serde_json::Value;
use tracing::{info, warn};

use crate::{
    error::GmpApiError,
    gmp_types::{
        CommonTaskFields, ConstructProofTask, ExecuteTask, GatewayTxTask, ReactToWasmEventTask,
        RefundTask, Task, VerifyTask,
    },
};

pub fn parse_task(task_json: &Value) -> Result<Task, GmpApiError> {
    let task_headers: CommonTaskFields = serde_json::from_value(task_json.clone())
        .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;

    // TODO: DRY
    match task_headers.r#type.as_str() {
        "CONSTRUCT_PROOF" => {
            let task: ConstructProofTask = serde_json::from_value(task_json.clone())
                .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;
            info!("Parsed ConstructProof task: {:?}", task);
            Ok(Task::ConstructProof(task))
        }
        "GATEWAY_TX" => {
            let task: GatewayTxTask = serde_json::from_value(task_json.clone())
                .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;
            info!("Parsed GatewayTx task: {:?}", task);
            Ok(Task::GatewayTx(task))
        }
        "VERIFY" => {
            let task: VerifyTask = serde_json::from_value(task_json.clone())
                .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;
            info!("Parsed VerifyTask task: {:?}", task);
            Ok(Task::Verify(task))
        }
        "EXECUTE" => {
            let task: ExecuteTask = serde_json::from_value(task_json.clone())
                .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;
            info!("Parsed Execute task: {:?}", task);
            Ok(Task::Execute(task))
        }
        "REFUND" => {
            let task: RefundTask = serde_json::from_value(task_json.clone())
                .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;
            info!("Parsed Refund task: {:?}", task);
            Ok(Task::Refund(task))
        }
        "REACT_TO_WASM_EVENT" => {
            let task: ReactToWasmEventTask = serde_json::from_value(task_json.clone())
                .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;
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
