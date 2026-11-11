use chrono::Utc;
use uuid::Uuid;
use exavault::{
    config::AppConfig,
    database::postgres::{create_postgres_pool, run_migrations},
    modules::vault::{dto::*, repository::VaultRepository, service::VaultService},
    pkg::crypto::{decrypt_secret, derive_master_key, encrypt_secret, hash_service_token},
};
use std::sync::Arc;
use tokio::sync::OnceCell;

static MIGRATED: OnceCell<()> = OnceCell::const_new();

async fn setup_test_context() -> Arc<VaultService> {
    let config = AppConfig::from_env();
    let pool = create_postgres_pool(&config.database_url)
        .await
        .expect("Failed to connect to postgres");

    MIGRATED
        .get_or_init(|| async {
            let _ = sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 9").execute(&pool).await;
            run_migrations(&pool).await.expect("Failed migrations");
        })
        .await;

    let repo = Arc::new(VaultRepository::new(pool));
    Arc::new(VaultService::new(repo, config.master_key_bytes()))
}

#[tokio::test]
async fn test_project_creation_and_auto_seed_environments() {
    let service = setup_test_context().await;

    let slug = format!("test-proj-{}", Utc::now().timestamp_micros());
    let req = CreateProjectRequest {
        name: "Test Automation Project".to_string(),
        slug: Some(slug.clone()),
        description: Some("Automated test project".to_string()),
        group_id: None,
        group_name: Some("Ecosystem Coffe".to_string()),
    };

    let created = service.create_project(req).await.expect("Failed to create project");
    assert_eq!(created.project.name, "Test Automation Project");
    assert_eq!(created.project.slug, slug);
    assert_eq!(created.project.group_name.as_deref(), Some("Ecosystem Coffe"));

    // Verify in list_projects
    let projs = service.list_projects().await.expect("Failed to list projects");
    let found = projs.iter().find(|p| p.project.id == created.project.id).expect("Project not found in list");
    assert_eq!(found.project.group_name.as_deref(), Some("Ecosystem Coffe"));

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
            group_id: None,
            group_name: None,
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

    service.bulk_upsert_secrets(dev_env.id, secrets, None).await.expect("Failed bulk upsert");

    // 2. Create Service Token
    let token_resp = service
        .create_service_token(
            dev_env.id,
            CreateServiceTokenRequest {
                name: "Test Runner Token".to_string(),
                expires_in_days: Some(30),
            },
            None,
        )
        .await
        .expect("Failed to create token");

    assert!(token_resp.token.starts_with("exv_"));

    // Verify audit logs were generated
    let logs = service.list_audit_logs(dev_env.id, Some(10)).await.expect("Failed to fetch audit logs");
    assert!(logs.iter().any(|l| l.action == "SAVE_SECRETS"), "Audit log for SAVE_SECRETS missing");
    assert!(logs.iter().any(|l| l.action == "CREATE_TOKEN"), "Audit log for CREATE_TOKEN missing");

    // 3. Export in dotenv format
    let (dotenv_out, content_type) = service.export_secrets(&token_resp.token, "dotenv", None, None).await.unwrap();
    assert_eq!(content_type, "text/plain; charset=utf-8");
    assert!(dotenv_out.contains("# Main DB"));
    assert!(dotenv_out.contains("DATABASE_URL=\"postgres://usr:p@ss@host:5432/db\""));
    assert!(dotenv_out.contains("APP_NAME=\"My \\\"Quoted\\\" App\""));
    assert!(dotenv_out.contains("MULTILINE_VAR=\"line1\\nline2\""));

    // 4. Export in shell format
    let (shell_out, _) = service.export_secrets(&token_resp.token, "shell", None, None).await.unwrap();
    assert!(shell_out.contains("export DATABASE_URL=\"postgres://usr:p@ss@host:5432/db\""));
    assert!(shell_out.contains("export APP_NAME=\"My \\\"Quoted\\\" App\""));

    // 5. Export in JSON format
    let (json_out, json_ct) = service.export_secrets(&token_resp.token, "json", None, None).await.unwrap();
    assert_eq!(json_ct, "application/json; charset=utf-8");
    let json_val: serde_json::Value = serde_json::from_str(&json_out).unwrap();
    assert_eq!(json_val["APP_NAME"], "My \"Quoted\" App");

    // Verify usage/export event was logged in audit trail
    let logs_after_export = service.list_audit_logs(dev_env.id, Some(10)).await.expect("Failed to fetch audit logs");
    assert!(logs_after_export.iter().any(|l| l.action == "EXPORT_SECRETS"), "Audit log for EXPORT_SECRETS missing");

    // 6. Test Cascading Delete
    service.delete_project(proj.project.id).await.unwrap();

    // Verify token lookup fails after cascade delete
    let token_hash = hash_service_token(&token_resp.token);
    let find_result = service.repository().find_token_by_hash(&token_hash).await;
    assert!(find_result.is_err());
}

#[tokio::test]
async fn test_user_management_and_project_rbac() {
    let service = setup_test_context().await;
    let ts = Utc::now().timestamp_micros();

    // 1. Create two test projects
    let p1 = service
        .create_project(CreateProjectRequest {
            name: format!("RBAC Project 1 {}", ts),
            slug: Some(format!("rbac-p1-{}", ts)),
            description: None,
            group_id: None,
            group_name: Some("RBAC Coffe".to_string()),
        })
        .await
        .unwrap();

    let p2 = service
        .create_project(CreateProjectRequest {
            name: format!("RBAC Project 2 {}", ts),
            slug: Some(format!("rbac-p2-{}", ts)),
            description: None,
            group_id: None,
            group_name: Some("RBAC Coffe".to_string()),
        })
        .await
        .unwrap();

    // 2. Create a team member user
    let user_email = format!("developer_{}@test.local", ts);
    let created_user = service
        .create_user(CreateVaultUserRequest {
            name: "John Developer".to_string(),
            email: user_email.clone(),
            password: "DevPassword123!".to_string(),
            role: Some("user".to_string()),
        })
        .await
        .expect("Failed to create user");

    assert_eq!(created_user.name, "John Developer");
    assert_eq!(created_user.role, "user");

    // Authenticate user check
    let auth_user = service
        .authenticate_user(&user_email, "DevPassword123!")
        .await
        .unwrap()
        .expect("User should authenticate successfully");

    assert_eq!(auth_user.id, created_user.id);

    // 3. Before assignment, user has 0 projects
    let user_projects = service.list_projects_for_user(&auth_user).await.unwrap();
    assert_eq!(user_projects.len(), 0);

    // 4. Assign user to p1 as "viewer"
    service
        .assign_project(created_user.id, p1.project.id, "viewer")
        .await
        .unwrap();

    let user_projects = service.list_projects_for_user(&auth_user).await.unwrap();
    assert_eq!(user_projects.len(), 1);
    assert_eq!(user_projects[0].project.id, p1.project.id);

    // 5. Check permissions on p1
    // "read" should be allowed
    let perm = service.check_project_permission(&auth_user, p1.project.id, "read").await;
    assert!(perm.is_ok());
    assert_eq!(perm.unwrap(), "viewer");

    // "editor" should fail because role is viewer
    let editor_perm = service.check_project_permission(&auth_user, p1.project.id, "editor").await;
    assert!(editor_perm.is_err());

    // Access to unassigned p2 should fail
    let p2_perm = service.check_project_permission(&auth_user, p2.project.id, "read").await;
    assert!(p2_perm.is_err());

    // 6. Upgrade user to "editor" on p1
    service
        .assign_project(created_user.id, p1.project.id, "editor")
        .await
        .unwrap();

    let editor_perm = service.check_project_permission(&auth_user, p1.project.id, "editor").await;
    assert!(editor_perm.is_ok());

    // 7. Global Admin has access to all projects
    let admin_user = VaultAuthUser {
        id: Uuid::nil(),
        username: "admin".to_string(),
        email: "admin@exavault.local".to_string(),
        role: "admin".to_string(),
    };

    let admin_p1_perm = service.check_project_permission(&admin_user, p1.project.id, "admin").await;
    assert!(admin_p1_perm.is_ok());

    let admin_p2_perm = service.check_project_permission(&admin_user, p2.project.id, "admin").await;
    assert!(admin_p2_perm.is_ok());

    // 8. Test password update and verification
    let fetched_user = service.get_user_by_id(created_user.id).await.unwrap();
    assert!(fetched_user.is_some());

    let new_password_hash = exavault::pkg::auth::password::hash_password("NewSecretPass456!").unwrap();
    service.update_user_password(created_user.id, &new_password_hash).await.unwrap();

    let old_auth = service.authenticate_user(&user_email, "DevPassword123!").await.unwrap();
    assert!(old_auth.is_none(), "Old password should no longer work");

    let new_auth = service.authenticate_user(&user_email, "NewSecretPass456!").await.unwrap();
    assert!(new_auth.is_some(), "New password should authenticate successfully");

    // 9. Delete user & verify project cleanup
    service.delete_user(created_user.id).await.unwrap();
    service.delete_project(p1.project.id).await.unwrap();
    service.delete_project(p2.project.id).await.unwrap();
}

#[tokio::test]
async fn test_first_class_ecosystem_groups_and_relational_join() {
    let service = setup_test_context().await;
    let ts = Utc::now().timestamp_micros();

    // 1. Create a first-class ecosystem group (e.g. "Fintech Platform")
    let group = service
        .create_group(CreateProjectGroupRequest {
            name: format!("Fintech Hub {}", ts),
            slug: Some(format!("fintech-hub-{}", ts)),
            description: Some("Banking and payments microservices".to_string()),
        })
        .await
        .expect("Failed to create ecosystem group");

    assert_eq!(group.name, format!("Fintech Hub {}", ts));

    // 2. Create projects linked to this group via foreign key group_id
    let p1 = service
        .create_project(CreateProjectRequest {
            name: format!("Payment Service {}", ts),
            slug: Some(format!("pay-svc-{}", ts)),
            description: None,
            group_id: Some(group.id),
            group_name: None,
        })
        .await
        .expect("Failed to create child project 1");

    let p2 = service
        .create_project(CreateProjectRequest {
            name: format!("Ledger Service {}", ts),
            slug: Some(format!("ledger-svc-{}", ts)),
            description: None,
            group_id: Some(group.id),
            group_name: None,
        })
        .await
        .expect("Failed to create child project 2");

    assert_eq!(p1.project.group_id, Some(group.id));
    assert_eq!(p1.project.group_name.as_deref(), Some(group.name.as_str()));
    assert_eq!(p2.project.group_id, Some(group.id));

    // 3. Verify group count in list_groups
    let groups = service.list_groups().await.expect("Failed to list groups");
    let found_group = groups.iter().find(|g| g.id == group.id).expect("Group not found");
    assert_eq!(found_group.projects_count, 2);

    // 4. Test Relational Power: Rename the group!
    let new_name = format!("Fintech Global {}", ts);
    let updated_group = service
        .update_group(
            group.id,
            UpdateProjectGroupRequest {
                name: Some(new_name.clone()),
                slug: None,
                description: None,
            },
        )
        .await
        .expect("Failed to update group");
    assert_eq!(updated_group.name, new_name);

    // Querying projects via JOIN now reflects the new name without touching project rows!
    let projs = service.list_projects().await.expect("Failed to list projects");
    let found_p1 = projs.iter().find(|p| p.project.id == p1.project.id).unwrap();
    assert_eq!(found_p1.project.group_name.as_deref(), Some(new_name.as_str()));

    // 5. Test ON DELETE SET NULL: Delete group leaves projects as standalone!
    service.delete_group(group.id).await.expect("Failed to delete group");
    let projs = service.list_projects().await.expect("Failed to list projects");
    let found_p1 = projs.iter().find(|p| p.project.id == p1.project.id).unwrap();
    assert_eq!(found_p1.project.group_id, None);
    assert_eq!(found_p1.project.group_name, None);

    // Cleanup projects
    service.delete_project(p1.project.id).await.unwrap();
    service.delete_project(p2.project.id).await.unwrap();
}

#[tokio::test]
async fn test_brand_assets_and_web_routes() {
    let favicon = include_str!("../public/favicon.svg");
    let logo = include_str!("../public/logo.svg");

    assert!(favicon.contains("<svg"));
    assert!(favicon.contains("core-glow"));
    assert!(logo.contains("<svg"));
    assert!(logo.contains("Exa"));
    assert!(logo.contains("Vault"));
    assert!(logo.contains("CRYPTOGRAPHIC SECRET ENGINE"));
}

#[tokio::test]
async fn test_webhook_crud_and_hmac_signature() {
    let service = setup_test_context().await;

    // 1. Test HMAC SHA-256 calculation
    let secret = "my-shared-hmac-secret";
    let payload_bytes = br#"{"event":"secrets_updated","project_slug":"backend"}"#;
    let sig = exavault::pkg::crypto::compute_hmac_sha256(payload_bytes, secret);
    assert!(sig.starts_with("sha256="));
    assert_eq!(sig.len(), 7 + 64); // "sha256=" prefix (7) + 64 hex characters

    // 2. Create a test project
    let slug = format!("test-wh-{}", Utc::now().timestamp_micros());
    let req = CreateProjectRequest {
        name: "Webhook Test Project".to_string(),
        slug: Some(slug.clone()),
        description: Some("Project for webhook testing".to_string()),
        group_id: None,
        group_name: None,
    };
    let created = service.create_project(req).await.expect("Failed to create project");
    let project_id = created.project.id;

    // 3. Create Webhook
    let wh_req = CreateWebhookRequest {
        name: "ExaForge CI Sync".to_string(),
        url: "https://exaforge.eka-dev.cloud/api/v1/webhooks/exavault".to_string(),
        secret_token: Some("secret123".to_string()),
        environment_id: None,
        events: Some(vec!["secrets.updated".to_string(), "secrets.deleted".to_string()]),
    };
    let webhook = service.create_webhook(project_id, wh_req).await.expect("Failed to create webhook");
    assert_eq!(webhook.name, "ExaForge CI Sync");
    assert_eq!(webhook.url, "https://exaforge.eka-dev.cloud/api/v1/webhooks/exavault");
    assert_eq!(webhook.secret_token.as_deref(), Some("secret123"));

    // 4. Test toggle active
    let toggled = service.toggle_webhook_active(webhook.id).await.expect("Failed to toggle");
    assert_eq!(toggled, false);
    let toggled_back = service.toggle_webhook_active(webhook.id).await.expect("Failed to toggle back");
    assert_eq!(toggled_back, true);

    // 5. List webhooks
    let list = service.list_webhooks(project_id).await.expect("Failed to list webhooks");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, webhook.id);

    // 6. Delete webhook
    service.delete_webhook(webhook.id).await.expect("Failed to delete webhook");
    let list_after = service.list_webhooks(project_id).await.expect("Failed to list webhooks");
    assert_eq!(list_after.len(), 0);

    // Cleanup project
    service.delete_project(project_id).await.unwrap();
}

#[tokio::test]
async fn test_auto_seed_admin_idempotency_and_auth() {
    let service = setup_test_context().await;

    // 1. Ensure initial admin is seeded if absent
    let _seeded = service
        .seed_default_admin("admin", "adminsecret123")
        .await
        .expect("Failed to run seed_default_admin");

    assert!(service.has_any_admin().await.unwrap());

    // 2. Subsequent runs of seed_default_admin MUST be idempotent and never overwrite existing admin
    let second_seed = service
        .seed_default_admin("admin", "completely-different-password")
        .await
        .expect("Idempotent seed check failed");
    assert!(!second_seed, "Should not seed again when an admin already exists");

    // 3. Test creating a dedicated admin user, authenticating, updating password, and cleanup
    let ts = Utc::now().timestamp_micros();
    let admin_name = format!("sec_admin_{}", ts);
    let admin_email = format!("sec_admin_{}@test.local", ts);
    let created_admin = service
        .create_user(CreateVaultUserRequest {
            name: admin_name.clone(),
            email: admin_email.clone(),
            password: "InitialAdminPass123!".to_string(),
            role: Some("admin".to_string()),
        })
        .await
        .expect("Failed to create test admin");

    assert_eq!(created_admin.role, "admin");

    // Authenticate by username AND email
    assert!(service.authenticate_user(&admin_name, "InitialAdminPass123!").await.unwrap().is_some());
    assert!(service.authenticate_user(&admin_email, "InitialAdminPass123!").await.unwrap().is_some());

    // Update password in DB and verify old password fails while new password succeeds
    let new_hash = exavault::pkg::auth::password::hash_password("UpdatedAdminPass999!").unwrap();
    service.update_user_password(created_admin.id, &new_hash).await.unwrap();

    assert!(service.authenticate_user(&admin_name, "InitialAdminPass123!").await.unwrap().is_none());
    let reauthed = service.authenticate_user(&admin_name, "UpdatedAdminPass999!").await.unwrap().unwrap();
    assert_eq!(reauthed.id, created_admin.id);
    assert_eq!(reauthed.role, "admin");

    // Cleanup test admin
    service.delete_user(created_admin.id).await.unwrap();
}
