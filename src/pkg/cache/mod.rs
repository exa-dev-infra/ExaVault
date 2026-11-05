use redis::{aio::MultiplexedConnection, AsyncCommands, Client};
use std::time::Duration;
use tracing::info;

use crate::pkg::errors::AppError;

#[derive(Clone)]
pub struct RedisCache {
    client: Client,
}

impl RedisCache {
    pub async fn new(redis_url: &str) -> Result<Self, AppError> {
        let client = Client::open(redis_url)
            .map_err(|e| AppError::internal(format!("Invalid Redis URL: {}", e)))?;

        // Verify connection with ping
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| AppError::internal(format!("Failed to connect to Redis: {}", e)))?;

        let pong: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| AppError::internal(format!("Redis PING failed: {}", e)))?;

        info!(ping = %pong, "Connected to Redis cache successfully");

        Ok(Self { client })
    }

    async fn get_conn(&self) -> Result<MultiplexedConnection, AppError> {
        self.client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| AppError::internal(format!("Redis connection error: {}", e)))
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        let mut conn = self.get_conn().await?;
        conn.get(key)
            .await
            .map_err(|e| AppError::internal(format!("Redis GET error: {}", e)))
    }

    pub async fn set(&self, key: &str, value: &str, ttl: Option<Duration>) -> Result<(), AppError> {
        let mut conn = self.get_conn().await?;
        if let Some(duration) = ttl {
            conn.set_ex::<_, _, ()>(key, value, duration.as_secs())
                .await
                .map_err(|e| AppError::internal(format!("Redis SETEX error: {}", e)))?;
        } else {
            conn.set::<_, _, ()>(key, value)
                .await
                .map_err(|e| AppError::internal(format!("Redis SET error: {}", e)))?;
        }
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        let mut conn = self.get_conn().await?;
        conn.del::<_, ()>(key)
            .await
            .map_err(|e| AppError::internal(format!("Redis DEL error: {}", e)))?;
        Ok(())
    }

    pub async fn exists(&self, key: &str) -> Result<bool, AppError> {
        let mut conn = self.get_conn().await?;
        conn.exists(key)
            .await
            .map_err(|e| AppError::internal(format!("Redis EXISTS error: {}", e)))
    }

    pub async fn health_check(&self) -> Result<(), AppError> {
        let mut conn = self.get_conn().await?;
        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| AppError::internal(format!("Redis health check failed: {}", e)))?;
        Ok(())
    }
}
