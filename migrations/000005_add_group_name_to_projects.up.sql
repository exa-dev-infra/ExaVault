ALTER TABLE projects ADD COLUMN IF NOT EXISTS group_name VARCHAR(100);
CREATE INDEX IF NOT EXISTS idx_projects_group_name ON projects(group_name);
