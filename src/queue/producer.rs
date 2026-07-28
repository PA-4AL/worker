use crate::errors::WorkerError;
use crate::models::WorkerResponse;
use google_cloud_googleapis::pubsub::v1::PubsubMessage;
use google_cloud_pubsub::publisher::Publisher;
use google_cloud_pubsub::topic::Topic;

pub struct QueueProducer {
    publisher: Publisher,
}

impl QueueProducer {
    pub fn new(topic: Topic) -> Self {
        Self {
            publisher: topic.new_publisher(None),
        }
    }

    pub async fn publish(&self, response: &WorkerResponse) -> Result<(), WorkerError> {
        let json =
            serde_json::to_string(response).map_err(|e| WorkerError::QueueError(e.to_string()))?;

        let msg = PubsubMessage {
            data: json.into_bytes(),
            ..Default::default()
        };

        let awaiter = self.publisher.publish(msg).await;
        awaiter
            .get()
            .await
            .map_err(|e| WorkerError::QueueError(e.to_string()))?;

        Ok(())
    }
}
