use super::{export_excel, import_excel};
use crate::errors::WorkerError;
use crate::models::{IncomingMessage, WorkerResponse};
use crate::retry::{execute_with_retry, RetryPolicy};

pub async fn dispatch(msg: &IncomingMessage) -> WorkerResponse {
    let policy = RetryPolicy::for_task(&msg.task_type);

    let result = execute_with_retry(&policy, || {
        let task_type = msg.task_type.clone();
        let payload = msg.payload.clone();
        async move {
            match task_type.as_str() {
                "import_excel" => import_excel::execute(payload).await,
                "export_excel" => export_excel::execute(payload).await,
                other => Err(WorkerError::UnknownTaskType(other.to_string())),
            }
        }
    })
    .await;

    match result {
        Ok(data) => WorkerResponse::success(msg.task_id.clone(), msg.task_type.clone(), data),
        Err((e, attempts)) => WorkerResponse::error(
            msg.task_id.clone(),
            msg.task_type.clone(),
            e.error_code().to_string(),
            e.to_string(),
            attempts,
        ),
    }
}
