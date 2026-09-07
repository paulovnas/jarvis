CREATE TABLE conversation_unread (
  conversation_id TEXT PRIMARY KEY NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  event_key TEXT NOT NULL,
  unread INTEGER NOT NULL DEFAULT 1,
  CONSTRAINT conversation_unread_flag CHECK (unread IN (0, 1))
);
