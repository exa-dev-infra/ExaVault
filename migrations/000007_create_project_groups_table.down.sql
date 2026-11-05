-- Re-add group_name column
ALTER TABLE projects ADD COLUMN IF NOT EXISTS group_name VARCHAR(100);

-- Restore group_name from joined project_groups table
UPDATE projects p
SET group_name = g.name
FROM project_groups g
WHERE p.group_id = g.id;

-- Drop foreign key and groups table
ALTER TABLE projects DROP COLUMN IF EXISTS group_id;
DROP TABLE IF EXISTS project_groups;
