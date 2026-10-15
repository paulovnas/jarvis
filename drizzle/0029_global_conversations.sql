ALTER TABLE conversations ADD COLUMN journal_project_id TEXT;

CREATE TABLE global_preferences (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    directory TEXT NOT NULL
);

DROP INDEX projects_path_unique;
CREATE UNIQUE INDEX projects_path_unique ON projects(path)
WHERE id <> '00000000000000000000000000000002';

CREATE TABLE navigation_selection_global (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    workspace_id TEXT REFERENCES workspaces(id),
    project_id TEXT REFERENCES projects(id),
    conversation_id TEXT REFERENCES conversations(id),
    CHECK (project_id IS NULL OR workspace_id IS NOT NULL)
);
INSERT INTO navigation_selection_global SELECT id, workspace_id, project_id, conversation_id FROM navigation_selection;
DROP TABLE navigation_selection;
ALTER TABLE navigation_selection_global RENAME TO navigation_selection;
