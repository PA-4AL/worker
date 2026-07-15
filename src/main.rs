use std::time::Duration;
use tokio_util::sync::CancellationToken;
use google_cloud_pubsub::client::{Client, ClientConfig};

mod config;
mod errors;
mod models;
mod parser;
mod queue;
mod retry;
mod tasks;

use config::Config;
use models::IncomingMessage;
use queue::{QueueConsumer, QueueProducer};
use tasks::dispatcher;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;
    tracing::info!("Config loaded, connecting to Pub/Sub...");

    let gcp_config = ClientConfig::default().with_auth().await?;
    let client = Client::new(gcp_config).await?;

    let subscription = client.subscription(&config.subscription_demands);
    let topic = client.topic(&config.topic_responses);

    let consumer = QueueConsumer::new(subscription);
    let producer = QueueProducer::new(topic);

    let shutdown = CancellationToken::new();

    // SIGINT (Ctrl+C)
    let shutdown_ctrlc = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("SIGINT received, shutting down...");
            shutdown_ctrlc.cancel();
        }
    });

    // SIGTERM (Docker / Kubernetes)
    #[cfg(unix)]
    {
        let shutdown_sigterm = shutdown.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            if let Ok(mut stream) = signal(SignalKind::terminate()) {
                stream.recv().await;
                tracing::info!("SIGTERM received, shutting down...");
                shutdown_sigterm.cancel();
            }
        });
    }

    tracing::info!(
        subscription = %config.subscription_demands,
        "Worker started, listening for messages"
    );

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("Graceful shutdown complete");
                break;
            }

            result = consumer.pull(10) => {
                match result {
                    Ok(messages) if messages.is_empty() => {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }

                    Ok(mut messages) => {
                        for msg in &mut messages {
                            let data = msg.message.data.clone();

                            match serde_json::from_slice::<IncomingMessage>(&data) {
                                Ok(incoming) => {
                                    tracing::info!(
                                        task_id = %incoming.task_id,
                                        task_type = %incoming.task_type,
                                        "Dispatching task"
                                    );

                                    let response = dispatcher::dispatch(&incoming).await;

                                    tracing::info!(
                                        task_id = %response.task_id,
                                        status = ?response.status,
                                        "Task done, publishing response"
                                    );

                                    if let Err(e) = producer.publish(&response).await {
                                        tracing::error!(error = %e, "Failed to publish response");
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(error = %e, "Cannot parse incoming message, skipping");
                                }
                            }

                            if let Err(e) = msg.ack().await {
                                tracing::error!(error = %e, "Failed to ack message");
                            }
                        }
                    }

                    Err(e) => {
                        tracing::error!(error = %e, "Failed to pull messages, retrying in 1s");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }
    }

    Ok(())
}
