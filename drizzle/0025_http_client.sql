CREATE TABLE http_settings (project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE CASCADE, payload TEXT NOT NULL);
CREATE TABLE http_requests (id TEXT PRIMARY KEY NOT NULL, project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE, payload TEXT NOT NULL);
CREATE INDEX http_requests_project ON http_requests(project_id);
CREATE TABLE http_drafts (id TEXT PRIMARY KEY NOT NULL, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE, payload TEXT NOT NULL);
CREATE INDEX http_drafts_conversation ON http_drafts(conversation_id);
CREATE TABLE http_runs (id TEXT PRIMARY KEY NOT NULL, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE, created_at INTEGER NOT NULL, payload TEXT NOT NULL);
CREATE INDEX http_runs_conversation ON http_runs(conversation_id, created_at DESC);
