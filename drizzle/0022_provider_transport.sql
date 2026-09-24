CREATE TABLE provider_transport_preferences (
  account_alias TEXT PRIMARY KEY NOT NULL REFERENCES provider_accounts(alias) ON DELETE CASCADE,
  incremental_responses INTEGER NOT NULL DEFAULT 0 CHECK (incremental_responses IN (0, 1))
);
