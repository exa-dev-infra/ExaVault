-- Revert phone and is_active columns from users table
DROP INDEX IF EXISTS idx_users_is_active;
ALTER TABLE users DROP COLUMN IF EXISTS is_active;
ALTER TABLE users DROP COLUMN IF EXISTS phone;

