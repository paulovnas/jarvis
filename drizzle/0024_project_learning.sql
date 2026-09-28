CREATE TABLE project_learning (
  project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  data TEXT NOT NULL CHECK (json_valid(data))
);
