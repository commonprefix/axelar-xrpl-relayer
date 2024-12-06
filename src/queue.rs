use anyhow::anyhow;
use lapin::{
    options::{BasicConsumeOptions, BasicPublishOptions, QueueDeclareOptions},
    types::FieldTable,
    BasicProperties, Connection, ConnectionProperties, Consumer,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{gmp_types::Task, subscriber::ChainTransaction};

#[derive(Clone)]
pub struct Queue {
    queue: lapin::Queue,
    channel: lapin::Channel,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum QueueItem {
    Task(Task),
    Transaction(ChainTransaction),
}

impl Queue {
    pub async fn new(url: &str, name: &str) -> Self {
        let connection = Connection::connect(url, ConnectionProperties::default())
            .await
            .unwrap();
        info!("Connected to RabbitMQ at {}", url);

        let channel = connection.create_channel().await.unwrap();
        info!("Created channel");

        // todo: test durable
        let q = channel
            .queue_declare(name, QueueDeclareOptions::default(), FieldTable::default())
            .await
            .unwrap();
        info!("Declared RMQ queue: {:?}", q);

        Self { queue: q, channel }
    }

    pub async fn publish(&self, msg: &[u8]) {
        let _ = self
            .channel
            .basic_publish(
                "",
                &self.queue.name().as_str(),
                BasicPublishOptions::default(),
                msg,
                BasicProperties::default(),
            )
            .await
            .unwrap();
    }

    pub async fn consumer(&self) -> Result<Consumer, anyhow::Error> {
        Ok(self
            .channel
            .basic_consume(
                self.queue.name().as_str(),
                "my consumer",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| anyhow!("Failed to create consumer: {:?}", e))?)
    }
}
