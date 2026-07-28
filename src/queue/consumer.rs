use crate::errors::WorkerError;
use google_cloud_pubsub::subscriber::ReceivedMessage;
use google_cloud_pubsub::subscription::Subscription;

pub struct QueueConsumer {
    subscription: Subscription,
}

impl QueueConsumer {
    pub fn new(subscription: Subscription) -> Self {
        Self { subscription }
    }

    /// Pull jusqu'à `max` messages. Retourne une liste vide si la queue est vide.
    pub async fn pull(&self, max: i32) -> Result<Vec<ReceivedMessage>, WorkerError> {
        self.subscription
            .pull(max, None)
            .await
            .map_err(|e| WorkerError::QueueError(e.to_string()))
    }
}
