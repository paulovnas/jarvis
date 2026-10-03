CREATE TABLE conversation_agent_models (
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    agent_key TEXT NOT NULL,
    choice TEXT NOT NULL,
    PRIMARY KEY (conversation_id, agent_key)
);
