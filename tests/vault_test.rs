use chrono::Utc;
use exavault::{
    config::AppConfig,
    database::postgres::{create_postgres_pool, run_migrations},
    modules::vault::{dto::*, repository::VaultRepository, service::VaultService},
    pkg::crypto::{decrypt_secret, derive_master_key, encrypt_secret, hash_service_token},
};
use std::sync::Arc;
use tokio::sync::OnceCell;

static SERVICE: OnceCell<Arc<VaultService>> = OnceCell::const_new();

async fn setup_test_context() -> Arc<VaultService> {
    SERVICE
        .get_or_init(|| async {
            let config = AppConfig::from_env();
            eprintln!("TEST CONFIG DATABASE_URL: {}", config.database_url);
            let pool = create_postgres_pool(&config.database_url)
                .await
                .expect("Failed to connect to postgres");
            run_migrations(&pool).await.expect("Failed migrations");

            let repo = Arc::new(VaultRepository::new(pool));
            Arc::new(VaultService::new(repo, config.master_key_bytes()))
        })
        .await
        .clone()
}

#[tokio::test]
async fn test_project_creation_and_auto_seed_environments() {
    let service = setup_test_context().await;

    let slug = format!("test-proj-{}", Utc::now().timestamp_micros());
    let req = CreateProjectRequest {
        name: "Test Automation Project".to_string(),
        slug: Some(slug.clone()),
        description: Some("Automated test project".to_string()),
    };

    let created = service.create_project(req).await.expect("Failed to create project");
    assert_eq!(created.project.name, "Test Automation Project");
    assert_eq!(created.project.slug, slug);

    // Environments dev, staging, prod must be auto-seeded
    assert_eq!(created.environments.len(), 3);
    let slugs: Vec<&str> = created.environments.iter().map(|e| e.slug.as_str()).collect();
    assert!(slugs.contains(&"dev"));
    assert!(slugs.contains(&"staging"));
    assert!(slugs.contains(&"prod"));

    // Cleanup: delete project
    service.delete_project(created.project.id).await.expect("Failed to delete project");
}

#[tokio::test]
async fn test_crypto_aes_gcm_unique_iv() {
    let key = derive_master_key("master_test_key_12345");
    let secret_val = "SECRET_VALUE_FOR_AES_GCM";

    let (c1, iv1, tag1) = encrypt_secret(secret_val, &key).expect("Enc 1 failed");
    let (c2, iv2, tag2) = encrypt_secret(secret_val, &key).expect("Enc 2 failed");

    // Every write operation must generate a unique 12-byte IV
    assert_ne!(iv1, iv2);
    assert_ne!(c1, c2); // Different IV produces different ciphertext even for same plaintext

    // Both must decrypt to the original secret
    let d1 = decrypt_secret(&c1, &iv1, &tag1, &key).expect("Dec 1 failed");
    let d2 = decrypt_secret(&c2, &iv2, &tag2, &key).expect("Dec 2 failed");
    assert_eq!(d1, secret_val);
    assert_eq!(d2, secret_val);
}

#[tokio::test]
async fn test_bulk_upsert_and_export_formats() {
    let service = setup_test_context().await;

    let proj_slug = format!("export-test-{}", Utc::now().timestamp_micros());
    let proj = service
        .create_project(CreateProjectRequest {
            name: "Export Test".to_string(),
            slug: Some(proj_slug),
            description: None,
        })
        .await
        .unwrap();

    let dev_env = proj.environments.iter().find(|e| e.slug == "dev").unwrap();

    // 1. Bulk upsert secrets
    let secrets = vec![
        UpsertSecretItem {
            key: "DATABASE_URL".to_string(),
            value: "postgres://usr:p@ss@host:5432/db".to_string(),
            comment: Some("Main DB".to_string()),
        },
        UpsertSecretItem {
            key: "APP_NAME".to_string(),
            value: "My \"Quoted\" App".to_string(),
            comment: None,
        },
        UpsertSecretItem {
            key: "MULTILINE_VAR".to_string(),
            value: "line1\nline2".to_string(),
            comment: Some("Multiline test".to_string()),
        },
    ];

    service.bulk_upsert_secrets(dev_env.id, secrets).await.expect("Failed bulk upsert");

    // 2. Create Service Token
    let token_resp = service
        .create_service_token(
            dev_env.id,
            CreateServiceTokenRequest {
                name: "Test Runner Token".to_string(),
                expires_in_days: Some(30),
            },
        )
        .await
        .expect("Failed to create token");

    assert!(token_resp.token.starts_with("exv_"));

    // 3. Export in dotenv format
    let (dotenv_out, content_type) = service.export_secrets(&token_resp.token, "dotenv").await.unwrap();
    assert_eq!(content_type, "text/plain; charset=utf-8");
    assert!(dotenv_out.contains("# Main DB"));
    assert!(dotenv_out.contains("DATABASE_URL=\"postgres://usr:p@ss@host:5432/db\""));
    assert!(dotenv_out.contains("APP_NAME=\"My \\\"Quoted\\\" App\""));
    assert!(dotenv_out.contains("MULTILINE_VAR=\"line1\\nline2\""));

    // 4. Export in shell format
    let (shell_out, _) = service.export_secrets(&token_resp.token, "shell").await.unwrap();
    assert!(shell_out.contains("export DATABASE_URL=\"postgres://usr:p@ss@host:5432/db\""));
    assert!(shell_out.contains("export APP_NAME=\"My \\\"Quoted\\\" App\""));

    // 5. Export in JSON format
    let (json_out, json_ct) = service.export_secrets(&token_resp.token, "json").await.unwrap();
    assert_eq!(json_ct, "application/json; charset=utf-8");
    let json_val: serde_json::Value = serde_json::from_str(&json_out).unwrap();
    assert_eq!(json_val["APP_NAME"], "My \"Quoted\" App");

    // 6. Test Cascading Delete
    service.delete_project(proj.project.id).await.unwrap();

    // Verify token lookup fails after cascade delete
    let token_hash = hash_service_token(&token_resp.token);
    let find_result = service.repository().find_token_by_hash(&token_hash).await;
    assert!(find_result.is_err());
}
