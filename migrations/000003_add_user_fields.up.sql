-- Add phone and is_active columns to users table for parity with Golang template
ALTER TABLE users ADD COLUMN IF NOT EXISTS phone VARCHAR(20);
ALTER TABLE users ADD COLUMN IF NOT EXISTS is_active BOOLEAN NOT NULL DEFAULT true;

-- Create index on is_active for faster active-user queries
CREATE INDEX IF NOT EXISTS idx_users_is_active ON users(is_active);

