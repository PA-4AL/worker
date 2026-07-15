use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("Missing column: {0}")]
    MissingColumn(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Invalid base64: {0}")]
    InvalidBase64(#[from] base64::DecodeError),

    #[error("Excel error: {0}")]
    ExcelError(String),

    #[error("Excel write error: {0}")]
    ExcelWriteError(String),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Queue error: {0}")]
    QueueError(String),

    #[error("Unknown task type: {0}")]
    UnknownTaskType(String),

    #[error("Invalid payload: {0}")]
    InvalidPayload(String),

    #[error("Config error: {0}")]
    ConfigError(String),
}

impl WorkerError {
    pub fn error_code(&self) -> &'static str {
        match self {
            WorkerError::MissingColumn(_) => "INVALID_SCHEMA",
            WorkerError::ParseError(_) => "PARSE_ERROR",
            WorkerError::InvalidBase64(_) => "INVALID_BASE64",
            WorkerError::ExcelError(_) => "EXCEL_ERROR",
            WorkerError::ExcelWriteError(_) => "EXCEL_WRITE_ERROR",
            WorkerError::JsonError(_) => "JSON_ERROR",
            WorkerError::QueueError(_) => "QUEUE_ERROR",
            WorkerError::UnknownTaskType(_) => "UNKNOWN_TASK_TYPE",
            WorkerError::InvalidPayload(_) => "INVALID_PAYLOAD",
            WorkerError::ConfigError(_) => "CONFIG_ERROR",
        }
    }
}
