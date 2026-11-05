use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    // Application & Server
    pub app_name: String,
    pub environment: String,
    pub host: String,
    pub port: u16,

    // Logging
    pub log_level: String,
    pub log_file: Option<String>,

    // Database
    pub database_url: String,

    // Redis
    pub redis_enabled: bool,
    pub redis_url: String,

    // RabbitMQ
    pub rabbitmq_enabled: bool,
    pub rabbitmq_url: String,

    // Feature Flags & Realtime Bridge
    pub realtime_enabled: bool,
    pub realtime_sse_enabled: bool,
    pub realtime_ws_enabled: bool,
    pub realtime_bridge: String, // "redis", "rabbitmq", "both", "local"

    // Idempotency
    pub idempotency_enabled: bool,
    pub idempotency_lifetime_hours: u64,
    pub idempotency_lock_timeout_seconds: u64,
    pub idempotency_header: String,

    // JWT & Auth
    pub jwt_secret: String,
    pub jwt_refresh_secret: String,
    pub access_token_expiry_minutes: i64,
    pub refresh_token_expiry_days: i64,
    pub jwt_rotation_threshold_days: i64,
    pub jwt_issuer: String,
    pub cookie_secure: bool,

    // OpenTelemetry
    pub otel_enabled: bool,
    pub otel_service_name: String,
    pub otel_exporter_endpoint: String,
    pub otel_exporter_protocol: String,
    pub otel_traces_sampler: String,
    pub otel_traces_sampler_arg: f64,

    // ExaVault
    pub exavault_master_key: String,
    pub admin_username: String,
    pub admin_password: String,
    pub app_url: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        // 1. Application & Server
        let app_name = env::var("APP_NAME").unwrap_or_else(|_| "template-rust-api".to_string());
        let environment = env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string());

        let host = env::var("SERVER_HOST")
            .or_else(|_| env::var("APP_HOST"))
            .unwrap_or_else(|_| "0.0.0.0".to_string());

        let port = env::var("SERVER_PORT")
            .or_else(|_| env::var("APP_PORT"))
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);

        // 2. Logging
        let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        let log_file = env::var("LOG_FILE").ok().filter(|s| !s.is_empty());

        // 3. Database URL (Construct from components or use raw DATABASE_URL)
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            let host = env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string());
            let port = env::var("DB_PORT").unwrap_or_else(|_| "5432".to_string());
            let user = env::var("DB_USER").unwrap_or_else(|_| "postgres".to_string());
            let pass = env::var("DB_PASSWORD").unwrap_or_else(|_| "postgres".to_string());
            let name = env::var("DB_NAME").unwrap_or_else(|_| "app_db".to_string());
            let ssl = env::var("DB_SSLMODE").unwrap_or_else(|_| "disable".to_string());
            format!(
                "postgres://{}:{}@{}:{}/{}?sslmode={}",
                user, pass, host, port, name, ssl
            )
        });

        // 4. Redis URL
        let redis_enabled = env::var("REDIS_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| {
            let host = env::var("REDIS_HOST").unwrap_or_else(|_| "localhost".to_string());
            let port = env::var("REDIS_PORT").unwrap_or_else(|_| "6379".to_string());
            let pass = env::var("REDIS_PASSWORD").unwrap_or_default();
            let db = env::var("REDIS_DB").unwrap_or_else(|_| "0".to_string());
            if pass.is_empty() {
                format!("redis://{}:{}/{}", host, port, db)
            } else {
                format!("redis://:{}@{}:{}/{}", pass, host, port, db)
            }
        });

        // 5. RabbitMQ URL
        let rabbitmq_enabled = env::var("RABBITMQ_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        let rabbitmq_url = env::var("RABBITMQ_URL").unwrap_or_else(|_| {
            let host = env::var("RABBITMQ_HOST").unwrap_or_else(|_| "localhost".to_string());
            let port = env::var("RABBITMQ_PORT").unwrap_or_else(|_| "5672".to_string());
            let user = env::var("RABBITMQ_USER").unwrap_or_else(|_| "guest".to_string());
            let pass = env::var("RABBITMQ_PASSWORD").unwrap_or_else(|_| "guest".to_string());
            let vhost = env::var("RABBITMQ_VHOST").unwrap_or_else(|_| "/".to_string());
            format!(
                "amqp://{}:{}@{}:{}/{}",
                user,
                pass,
                host,
                port,
                urlencoding(vhost)
            )
        });

        // 6. Realtime Feature Flags & Bridge
        let realtime_enabled = env::var("REALTIME_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true);

        let realtime_sse_enabled = env::var("REALTIME_SSE_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true);

        let realtime_ws_enabled = env::var("REALTIME_WS_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true);

        let realtime_bridge = env::var("REALTIME_BRIDGE")
            .unwrap_or_else(|_| "local".to_string())
            .to_lowercase();

        // 7. Idempotency Flags
        let idempotency_enabled = env::var("IDEMPOTENCY_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true);

        let idempotency_lifetime_hours = env::var("IDEMPOTENCY_LIFETIME_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(24);

        let idempotency_lock_timeout_seconds = env::var("IDEMPOTENCY_LOCK_TIMEOUT_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let idempotency_header =
            env::var("IDEMPOTENCY_HEADER").unwrap_or_else(|_| "Idempotency-Key".to_string());

        // 8. JWT & Auth
        let jwt_secret = env::var("JWT_ACCESS_SECRET")
            .or_else(|_| env::var("JWT_SECRET"))
            .unwrap_or_else(|_| {
                "super-secret-jwt-key-replace-this-in-production-12345".to_string()
            });

        let jwt_refresh_secret = env::var("JWT_REFRESH_SECRET").unwrap_or_else(|_| {
            "another-secret-refresh-key-replace-this-in-production-12345".to_string()
        });

        let access_token_expiry_minutes = env::var("JWT_ACCESS_DURATION")
            .or_else(|_| env::var("JWT_ACCESS_EXPIRY_MINUTES"))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(15);

        let refresh_token_expiry_days = env::var("JWT_REFRESH_DURATION")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .map(|hours| hours / 24)
            .or_else(|| {
                env::var("JWT_REFRESH_EXPIRY_DAYS")
                    .ok()
                    .and_then(|v| v.parse().ok())
            })
            .unwrap_or(7);

        let jwt_rotation_threshold_days = env::var("JWT_ROTATION_THRESHOLD_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);

        let jwt_issuer = env::var("JWT_ISSUER").unwrap_or_else(|_| "template-rust".to_string());

        let cookie_secure = env::var("COOKIE_SECURE")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        // 9. OpenTelemetry
        let otel_enabled = env::var("OTEL_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        let otel_service_name = env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| app_name.clone());

        let otel_exporter_endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:4317".to_string());

        let otel_exporter_protocol =
            env::var("OTEL_EXPORTER_OTLP_PROTOCOL").unwrap_or_else(|_| "grpc".to_string());

        let otel_traces_sampler = env::var("OTEL_TRACES_SAMPLER")
            .unwrap_or_else(|_| "parentbased_traceidratio".to_string());

        let otel_traces_sampler_arg = env::var("OTEL_TRACES_SAMPLER_ARG")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.2);

        // 10. ExaVault
        let exavault_master_key = env::var("EXAVAULT_MASTER_KEY").unwrap_or_else(|_| {
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()
        });
        let admin_username = env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string());
        let admin_password = env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "adminsecret123".to_string());
        let app_url = env::var("APP_URL").unwrap_or_else(|_| format!("http://{}:{}", host, port));

        Self {
            app_name,
            environment,
            host,
            port,
            log_level,
            log_file,
            database_url,
            redis_enabled,
            redis_url,
            rabbitmq_enabled,
            rabbitmq_url,
            realtime_enabled,
            realtime_sse_enabled,
            realtime_ws_enabled,
            realtime_bridge,
            idempotency_enabled,
            idempotency_lifetime_hours,
            idempotency_lock_timeout_seconds,
            idempotency_header,
            jwt_secret,
            jwt_refresh_secret,
            access_token_expiry_minutes,
            refresh_token_expiry_days,
            jwt_rotation_threshold_days,
            jwt_issuer,
            cookie_secure,
            otel_enabled,
            otel_service_name,
            otel_exporter_endpoint,
            otel_exporter_protocol,
            otel_traces_sampler,
            otel_traces_sampler_arg,
            exavault_master_key,
            admin_username,
            admin_password,
            app_url,
        }
    }

    pub fn master_key_bytes(&self) -> [u8; 32] {
        crate::pkg::crypto::derive_master_key(&self.exavault_master_key)
    }
}

fn urlencoding(s: String) -> String {
    if s == "/" {
        "%2f".to_string()
    } else {
        s
    }
}
