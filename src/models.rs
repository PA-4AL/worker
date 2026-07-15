use serde::{Deserialize, Serialize};

/// Message reçu depuis la queue "topic-demandes" (Backend → Worker)
#[derive(Debug, Deserialize)]
pub struct IncomingMessage {
    pub task_id: String,
    pub task_type: String,
    pub payload: serde_json::Value,
}

/// Réponse publiée dans "topic-réponses" (Worker → Backend)
#[derive(Debug, Serialize)]
pub struct WorkerResponse {
    pub task_id: String,
    pub task_type: String,
    pub status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetail>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Success,
    Error,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    pub attempts: u32,
}

impl WorkerResponse {
    pub fn success(task_id: String, task_type: String, data: serde_json::Value) -> Self {
        Self {
            task_id,
            task_type,
            status: ResponseStatus::Success,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(
        task_id: String,
        task_type: String,
        code: String,
        message: String,
        attempts: u32,
    ) -> Self {
        Self {
            task_id,
            task_type,
            status: ResponseStatus::Error,
            data: None,
            error: Some(ErrorDetail { code, message, attempts }),
        }
    }
}
