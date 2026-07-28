use crate::errors::WorkerError;
use std::future::Future;
use std::time::Duration;

pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_delay: Duration,
}

impl RetryPolicy {
    pub fn for_task(task_type: &str) -> Self {
        match task_type {
            "import_excel" => Self {
                max_attempts: 3,
                initial_delay: Duration::from_secs(1),
            },
            // Génération de fichier purement locale et déterministe :
            // un échec se reproduirait à l'identique, inutile de rejouer.
            "export_excel" => Self {
                max_attempts: 1,
                initial_delay: Duration::ZERO,
            },
            _ => Self {
                max_attempts: 1,
                initial_delay: Duration::ZERO,
            },
        }
    }

    /// Backoff exponentiel : 0s, 1s, 2s, 4s, ...
    fn backoff_delay(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }
        self.initial_delay * 2u32.pow(attempt - 1)
    }
}

/// Exécute `f` jusqu'à `policy.max_attempts` fois avec backoff exponentiel.
/// Retourne `Ok(data)` dès le premier succès, ou `Err((erreur, nb_tentatives))`.
pub async fn execute_with_retry<F, Fut>(
    policy: &RetryPolicy,
    mut f: F,
) -> Result<serde_json::Value, (WorkerError, u32)>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<serde_json::Value, WorkerError>>,
{
    let mut last_error: Option<WorkerError> = None;

    for attempt in 0..policy.max_attempts {
        let delay = policy.backoff_delay(attempt);
        if delay > Duration::ZERO {
            tokio::time::sleep(delay).await;
        }

        match f().await {
            Ok(data) => return Ok(data),
            Err(e) => {
                tracing::warn!(
                    attempt = attempt + 1,
                    max = policy.max_attempts,
                    error = %e,
                    "Task attempt failed"
                );
                last_error = Some(e);
            }
        }
    }

    match last_error {
        Some(e) => Err((e, policy.max_attempts)),
        None => Err((
            WorkerError::ParseError("max_attempts must be > 0".into()),
            0,
        )),
    }
}
