use lapin::{
    options::{BasicPublishOptions, QueueDeclareOptions},
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties,
};
use std::sync::Arc;
use tracing::info;

use crate::pkg::errors::AppError;

#[derive(Clone)]
pub struct RabbitMQBroker {
    connection: Arc<Connection>,
    channel: Arc<Channel>,
}

impl RabbitMQBroker {
    pub async fn new(amqp_url: &str) -> Result<Self, AppError> {
        let conn = Connection::connect(amqp_url, ConnectionProperties::default())
            .await
            .map_err(|e| AppError::internal(format!("RabbitMQ connection failed: {}", e)))?;

        let ch = conn
            .create_channel()
            .await
            .map_err(|e| AppError::internal(format!("RabbitMQ channel creation failed: {}", e)))?;

        info!("Connected to RabbitMQ broker successfully");

        Ok(Self {
            connection: Arc::new(conn),
            channel: Arc::new(ch),
        })
    }

    pub async fn declare_queue(&self, name: &str, durable: bool) -> Result<(), AppError> {
        self.channel
            .queue_declare(
                name,
                QueueDeclareOptions {
                    durable,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| AppError::internal(format!("Queue declare error: {}", e)))?;

        Ok(())
    }

    pub async fn publish(
        &self,
        exchange: &str,
        routing_key: &str,
        payload: &[u8],
    ) -> Result<(), AppError> {
        self.channel
            .basic_publish(
                exchange,
                routing_key,
                BasicPublishOptions::default(),
                payload,
                BasicProperties::default().with_content_type("application/json".into()),
            )
            .await
            .map_err(|e| AppError::internal(format!("RabbitMQ publish error: {}", e)))?
            .await
            .map_err(|e| AppError::internal(format!("RabbitMQ confirm error: {}", e)))?;

        Ok(())
    }

    pub async fn health_check(&self) -> Result<(), AppError> {
        if self.connection.status().connected() {
            Ok(())
        } else {
            Err(AppError::internal("RabbitMQ connection is closed"))
        }
    }
}
