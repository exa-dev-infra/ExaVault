-- ExaVault Core Tables

-- 1. Projects table
CREATE TABLE IF NOT EXISTS projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 2. Environments table (dev, staging, prod, etc.)
CREATE TABLE IF NOT EXISTS environments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name VARCHAR(50) NOT NULL,
    slug VARCHAR(50) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_project_env_slug UNIQUE(project_id, slug)
);

-- 3. Secrets table (Encrypted at rest with AES-256-GCM)
CREATE TABLE IF NOT EXISTS secrets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    key VARCHAR(255) NOT NULL,
    encrypted_value TEXT NOT NULL,
    iv VARCHAR(64) NOT NULL,
    auth_tag VARCHAR(64) NOT NULL,
    comment TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_env_secret_key UNIQUE(environment_id, key)
);

-- 4. Service Tokens table (CI/CD & container machine auth)
CREATE TABLE IF NOT EXISTS service_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    token_prefix VARCHAR(32) NOT NULL,
    token_hash VARCHAR(64) NOT NULL UNIQUE,
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Performance Indexes
CREATE INDEX IF NOT EXISTS idx_environments_project_id ON environments(project_id);
CREATE INDEX IF NOT EXISTS idx_secrets_environment_id ON secrets(environment_id);
CREATE INDEX IF NOT EXISTS idx_secrets_env_key ON secrets(environment_id, key);
CREATE INDEX IF NOT EXISTS idx_service_tokens_env_id ON service_tokens(environment_id);
CREATE INDEX IF NOT EXISTS idx_service_tokens_hash ON service_tokens(token_hash);

