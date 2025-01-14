use std::sync::Arc;

use anyhow::anyhow;
use lapin::{
    options::{BasicConsumeOptions, BasicPublishOptions, QueueDeclareOptions},
    types::FieldTable,
    BasicProperties, Connection, ConnectionProperties, Consumer,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::{gmp_api::gmp_types::Task, subscriber::ChainTransaction};

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
    pub async fn new(url: &str, name: &str) -> Arc<Self> {
        let (connection, channel, queue) = Self::connect(url, name).await;

        let queue_arc = Arc::new(Self { channel, queue });

        Self::set_on_error_callback(connection, url.to_owned(), name.to_owned());

        queue_arc
    }

    pub async fn connect(url: &str, name: &str) -> (Connection, lapin::Channel, lapin::Queue) {
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

        (connection, channel, q)
    }

    fn set_on_error_callback(connection: Connection, url: String, name: String) {
        connection.on_error(move |_| {
            error!("Connection to RabbitMQ failed");
            std::process::exit(1);
        });
    }

    pub async fn refresh_connection(&mut self, url: &str, name: &str) {
        info!("Reconnecting to RabbitMQ at {}", url);
        let (connection, channel, queue) = Self::connect(url, name).await;
        self.channel = channel;
        self.queue = queue;

        Self::set_on_error_callback(connection, url.to_owned(), name.to_owned());

        info!("Reconnected to RabbitMQ at {}", url);
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
        let consumer_tag = format!("consumer_{}", Uuid::new_v4());

        Ok(self
            .channel
            .basic_consume(
                self.queue.name().as_str(),
                &consumer_tag,
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| anyhow!("Failed to create consumer: {:?}", e))?)
    }
}
