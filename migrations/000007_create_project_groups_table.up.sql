-- 1. Create first-class project_groups table
CREATE TABLE IF NOT EXISTS project_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) UNIQUE NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_project_groups_slug ON project_groups(slug);

-- 2. Backfill groups from existing distinct group_names
INSERT INTO project_groups (name, slug)
SELECT DISTINCT group_name, LOWER(REGEXP_REPLACE(TRIM(group_name), '[^a-zA-Z0-9]+', '-', 'g'))
FROM projects
WHERE group_name IS NOT NULL AND TRIM(group_name) <> ''
ON CONFLICT (slug) DO NOTHING;

-- 3. Add group_id foreign key to projects table
ALTER TABLE projects ADD COLUMN IF NOT EXISTS group_id UUID REFERENCES project_groups(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_projects_group_id ON projects(group_id);

-- 4. Link existing projects to their project_group by name
UPDATE projects p
SET group_id = g.id
FROM project_groups g
WHERE p.group_name = g.name;

-- 5. Drop redundant loose text column group_name
ALTER TABLE projects DROP COLUMN IF EXISTS group_name;

