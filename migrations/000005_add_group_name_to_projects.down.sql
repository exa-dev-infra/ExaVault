DROP INDEX IF EXISTS idx_projects_group_name;
ALTER TABLE projects DROP COLUMN IF EXISTS group_name;
