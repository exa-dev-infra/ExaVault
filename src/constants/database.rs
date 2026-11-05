//! Database constraint definitions and mappings.
//! Maps PostgreSQL constraint names to human-readable field names and semantic error codes.
//! Developers can define and customize constraint mappings here!

use crate::constants::codes::{CODE_CONFLICT, CODE_EMAIL_ALREADY_EXISTS};

/// Explicit mapping of PostgreSQL UNIQUE constraint names to (field_name, semantic_error_code).
/// Format: `(constraint_name, field_display_name, semantic_error_code)`
pub const UNIQUE_CONSTRAINTS: &[(&str, &str, &str)] = &[
    ("users_email_key", "email", CODE_EMAIL_ALREADY_EXISTS),
    ("users_phone_key", "phone", CODE_CONFLICT),
    ("users_username_key", "username", CODE_CONFLICT),
    (
        "refresh_tokens_token_hash_key",
        "refresh token",
        CODE_CONFLICT,
    ),
    ("skills_name_key", "name", CODE_CONFLICT),
    ("products_sku_key", "SKU", CODE_CONFLICT),
    ("products_slug_key", "slug", CODE_CONFLICT),
    ("categories_name_key", "name", CODE_CONFLICT),
    ("categories_slug_key", "slug", CODE_CONFLICT),
    ("roles_name_key", "role name", CODE_CONFLICT),
    ("permissions_name_key", "permission name", CODE_CONFLICT),
    ("projects_slug_key", "project slug", CODE_CONFLICT),
    ("unique_project_env_slug", "environment slug", CODE_CONFLICT),
    ("unique_env_secret_key", "secret key", CODE_CONFLICT),
    ("service_tokens_token_hash_key", "service token hash", CODE_CONFLICT),
];

/// Explicit mapping of PostgreSQL FOREIGN KEY constraint names to referenced entity names.
/// Format: `(constraint_name, referenced_entity_name)`
pub const FOREIGN_KEY_CONSTRAINTS: &[(&str, &str)] = &[
    ("refresh_tokens_user_id_fkey", "user"),
    ("users_role_id_fkey", "role"),
    ("orders_user_id_fkey", "user"),
    ("order_items_order_id_fkey", "order"),
    ("order_items_product_id_fkey", "product"),
    ("products_category_id_fkey", "category"),
    ("user_permissions_user_id_fkey", "user"),
    ("user_permissions_permission_id_fkey", "permission"),
    ("environments_project_id_fkey", "project"),
    ("secrets_environment_id_fkey", "environment"),
    ("service_tokens_environment_id_fkey", "environment"),
];

/// Looks up an explicit unique constraint mapping by constraint name.
pub fn lookup_unique_constraint(constraint_name: &str) -> Option<(&'static str, &'static str)> {
    UNIQUE_CONSTRAINTS
        .iter()
        .find(|(name, _, _)| *name == constraint_name)
        .map(|(_, field, code)| (*field, *code))
}

/// Looks up an explicit foreign key constraint mapping by constraint name.
pub fn lookup_foreign_key_constraint(constraint_name: &str) -> Option<&'static str> {
    FOREIGN_KEY_CONSTRAINTS
        .iter()
        .find(|(name, _)| *name == constraint_name)
        .map(|(_, entity)| *entity)
}
