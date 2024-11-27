use anyhow::anyhow;
use lapin::{
    options::{BasicConsumeOptions, BasicPublishOptions, QueueDeclareOptions},
    types::FieldTable,
    BasicProperties, Connection, ConnectionProperties, Consumer,
};
use tracing::info;

pub struct Queue {
    queue: lapin::Queue,
    channel: lapin::Channel,
    consumer: Option<lapin::Consumer>,
}

impl Queue {
    pub async fn new(url: &str) -> Self {
        let connection = Connection::connect(url, ConnectionProperties::default())
            .await
            .unwrap();
        info!("Connected to RabbitMQ at {}", url);

        // Create channel
        let channel = connection.create_channel().await.unwrap();
        info!("Created channel");

        // Create Q
        let queue_name = "testqueue";
        // todo: test durable
        let q = channel
            .queue_declare(
                queue_name,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
            .unwrap();
        info!("Declared RMQ queue: {:?}", q);

        Self {
            queue: q,
            channel,
            consumer: None,
        }
    }

    pub async fn publish(&self, msg: &[u8]) {
        let confirm = self
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
        info!("Published message: {:?}", confirm);
    }

    pub async fn consumer(&mut self) -> Result<&mut Consumer, anyhow::Error> {
        if self.consumer.is_none() {
            self.consumer = Some(
                self.channel
                    .basic_consume(
                        self.queue.name().as_str(),
                        "my consumer",
                        BasicConsumeOptions::default(),
                        FieldTable::default(),
                    )
                    .await
                    .map_err(|e| anyhow!("Failed to create consumer: {:?}", e))?,
            )
        }
        Ok(self.consumer.as_mut().unwrap())
    }
}
