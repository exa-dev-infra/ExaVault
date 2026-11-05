use async_trait::async_trait;
use futures_util::StreamExt;
use lapin::{
    options::{
        BasicConsumeOptions, BasicPublishOptions, ExchangeDeclareOptions, QueueBindOptions,
        QueueDeclareOptions,
    },
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::config::AppConfig;

pub const BRIDGE_REDIS: &str = "redis";
pub const BRIDGE_RABBITMQ: &str = "rabbitmq";
pub const BRIDGE_BOTH: &str = "both";
pub const BRIDGE_LOCAL: &str = "local";

const RABBITMQ_REALTIME_EXCHANGE: &str = "realtime.events";

#[async_trait]
pub trait RealtimeBridge: Send + Sync {
    fn name(&self) -> &str;
    async fn publish(
        &self,
        channel: &str,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Fallback local-only bridge when no distributed broker is used.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalBridge;

impl LocalBridge {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RealtimeBridge for LocalBridge {
    fn name(&self) -> &str {
        BRIDGE_LOCAL
    }

    async fn publish(
        &self,
        _channel: &str,
        _message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// Redis pub/sub distributed realtime bridge.
pub struct RedisBridge {
    client: redis::Client,
}

impl RedisBridge {
    pub async fn new(
        redis_url: &str,
        hub_sender: broadcast::Sender<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = redis::Client::open(redis_url)?;

        // Spawn background listener task for Redis pub/sub
        let listener_client = client.clone();
        tokio::spawn(async move {
            if let Ok(mut pubsub) = listener_client.get_async_pubsub().await {
                if let Err(e) = pubsub.psubscribe("realtime:*").await {
                    error!(error = %e, "Failed to psubscribe to realtime:* in Redis");
                    return;
                }
                info!("Realtime Redis bridge listening on topic realtime:*");

                let mut stream = pubsub.on_message();
                while let Some(msg) = stream.next().await {
                    if let Ok(payload) = msg.get_payload::<String>() {
                        let _ = hub_sender.send(payload);
                    }
                }
            } else {
                warn!("Could not connect to Redis for realtime bridge pubsub listener");
            }
        });

        Ok(Self { client })
    }
}

#[async_trait]
impl RealtimeBridge for RedisBridge {
    fn name(&self) -> &str {
        BRIDGE_REDIS
    }

    async fn publish(
        &self,
        channel: &str,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let topic = if channel.is_empty() || channel == "broadcast" {
            "realtime:broadcast".to_string()
        } else {
            format!("realtime:channel:{}", channel)
        };

        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = redis::cmd("PUBLISH")
            .arg(&topic)
            .arg(message)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }
}

/// RabbitMQ fanout exchange distributed realtime bridge.
pub struct RabbitMQBridge {
    channel: Channel,
}

impl RabbitMQBridge {
    pub async fn new(
        amqp_url: &str,
        hub_sender: broadcast::Sender<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let conn = Connection::connect(amqp_url, ConnectionProperties::default()).await?;
        let channel = conn.create_channel().await?;

        // 1. Declare fanout exchange
        channel
            .exchange_declare(
                RABBITMQ_REALTIME_EXCHANGE,
                ExchangeKind::Fanout,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        // 2. Declare exclusive, auto-delete queue for this instance
        let queue = channel
            .queue_declare(
                "",
                QueueDeclareOptions {
                    exclusive: true,
                    auto_delete: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        // 3. Bind queue to fanout exchange
        channel
            .queue_bind(
                queue.name().as_str(),
                RABBITMQ_REALTIME_EXCHANGE,
                "",
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await?;

        // 4. Background consumer for this pod's exclusive queue
        let queue_name = queue.name().to_string();
        let consume_channel = channel.clone();
        tokio::spawn(async move {
            if let Ok(mut consumer) = consume_channel
                .basic_consume(
                    &queue_name,
                    "realtime_bridge_consumer",
                    BasicConsumeOptions {
                        no_ack: true,
                        ..Default::default()
                    },
                    FieldTable::default(),
                )
                .await
            {
                info!("Realtime RabbitMQ bridge consumer listening on fanout exchange");
                while let Some(delivery_res) = consumer.next().await {
                    if let Ok(delivery) = delivery_res {
                        if let Ok(text) = String::from_utf8(delivery.data) {
                            let _ = hub_sender.send(text);
                        }
                    }
                }
            }
        });

        Ok(Self { channel })
    }
}

#[async_trait]
impl RealtimeBridge for RabbitMQBridge {
    fn name(&self) -> &str {
        BRIDGE_RABBITMQ
    }

    async fn publish(
        &self,
        _channel: &str,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.channel
            .basic_publish(
                RABBITMQ_REALTIME_EXCHANGE,
                "",
                BasicPublishOptions::default(),
                message.as_bytes(),
                BasicProperties::default().with_content_type("application/json".into()),
            )
            .await?
            .await?;

        Ok(())
    }
}

/// Dual bridge publishing to both Redis and RabbitMQ.
pub struct BothBridge {
    redis: Arc<dyn RealtimeBridge>,
    rabbit: Arc<dyn RealtimeBridge>,
}

#[async_trait]
impl RealtimeBridge for BothBridge {
    fn name(&self) -> &str {
        BRIDGE_BOTH
    }

    async fn publish(
        &self,
        channel: &str,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _ = self.redis.publish(channel, message).await;
        let _ = self.rabbit.publish(channel, message).await;
        Ok(())
    }
}

/// Factory function to initialize the configured realtime bridge with graceful degradation.
pub async fn create_realtime_bridge(
    config: &AppConfig,
    hub_sender: broadcast::Sender<String>,
) -> Arc<dyn RealtimeBridge> {
    if !config.realtime_enabled {
        return Arc::new(LocalBridge);
    }

    match config.realtime_bridge.as_str() {
        BRIDGE_REDIS => match RedisBridge::new(&config.redis_url, hub_sender).await {
            Ok(bridge) => {
                info!("Distributed realtime bridge initialized via Redis pub/sub");
                Arc::new(bridge)
            }
            Err(err) => {
                warn!(
                    error = %err,
                    "Failed to connect Redis realtime bridge. Falling back to Local in-memory mode."
                );
                Arc::new(LocalBridge)
            }
        },
        BRIDGE_RABBITMQ => match RabbitMQBridge::new(&config.rabbitmq_url, hub_sender).await {
            Ok(bridge) => {
                info!("Distributed realtime bridge initialized via RabbitMQ fanout exchange");
                Arc::new(bridge)
            }
            Err(err) => {
                warn!(
                    error = %err,
                    "Failed to connect RabbitMQ realtime bridge. Falling back to Local in-memory mode."
                );
                Arc::new(LocalBridge)
            }
        },
        BRIDGE_BOTH => {
            let redis_res = RedisBridge::new(&config.redis_url, hub_sender.clone()).await;
            let rabbit_res = RabbitMQBridge::new(&config.rabbitmq_url, hub_sender).await;

            match (redis_res, rabbit_res) {
                (Ok(red), Ok(rab)) => {
                    info!("Distributed realtime bridge initialized via BOTH Redis and RabbitMQ");
                    Arc::new(BothBridge {
                        redis: Arc::new(red),
                        rabbit: Arc::new(rab),
                    })
                }
                _ => {
                    warn!(
                        "Failed to initialize both brokers. Falling back to Local in-memory mode."
                    );
                    Arc::new(LocalBridge)
                }
            }
        }
        _ => {
            info!("Realtime hub running in single-instance local in-memory mode");
            Arc::new(LocalBridge)
        }
    }
}
