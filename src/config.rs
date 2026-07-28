use crate::errors::WorkerError;

#[derive(Debug, Clone)]
pub struct Config {
    pub gcp_project_id: String,
    pub subscription_demands: String,
    pub topic_responses: String,
    /// Port de la sonde HTTP — `PORT` est injecté par Cloud Run (8080 par défaut).
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self, WorkerError> {
        dotenvy::dotenv().ok();

        let gcp_project_id = std::env::var("GCP_PROJECT_ID")
            .map_err(|_| WorkerError::ConfigError("GCP_PROJECT_ID not set".into()))?;

        let subscription_demands = std::env::var("PUBSUB_SUBSCRIPTION_DEMANDS")
            .map_err(|_| WorkerError::ConfigError("PUBSUB_SUBSCRIPTION_DEMANDS not set".into()))?;

        let topic_responses = std::env::var("PUBSUB_TOPIC_RESPONSES")
            .map_err(|_| WorkerError::ConfigError("PUBSUB_TOPIC_RESPONSES not set".into()))?;

        let port = match std::env::var("PORT") {
            Ok(value) => value
                .parse()
                .map_err(|_| WorkerError::ConfigError(format!("PORT invalide : {value}")))?,
            Err(_) => 8080,
        };

        Ok(Self {
            gcp_project_id,
            subscription_demands,
            topic_responses,
            port,
        })
    }
}
