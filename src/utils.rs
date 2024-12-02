use serde_json::Value;
use tracing::{info, warn};

use crate::{
    error::GmpApiError,
    gmp_types::{CommonTaskFields, ExecuteTask, RefundTask, Task},
};

pub fn parse_task(task_json: &Value) -> Result<Task, GmpApiError> {
    let task_headers: CommonTaskFields = serde_json::from_value(task_json.clone())
        .map_err(|e| GmpApiError::InvalidResponse(e.to_string()))?;

    match task_headers.r#type.as_str() {
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
        _ => {
            warn!("Unknown task type: {:?}", task_headers.r#type);
            Err(GmpApiError::InvalidResponse(
                "Unknown task type".to_string(),
            ))
        }
    }
}
