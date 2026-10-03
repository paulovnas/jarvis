-- Keep dependent provider preferences when expanding the built-in kinds.
CREATE TEMP TABLE saved_go_custom AS SELECT * FROM custom_provider_configs;
CREATE TEMP TABLE saved_go_alerts AS SELECT * FROM provider_usage_alert_deliveries;
CREATE TEMP TABLE saved_go_models AS SELECT * FROM provider_model_exclusions;
CREATE TEMP TABLE saved_go_transport AS SELECT * FROM provider_transport_preferences;
CREATE TABLE provider_accounts_next (
  alias TEXT PRIMARY KEY NOT NULL,
  provider_kind TEXT NOT NULL CHECK (provider_kind IN ('openai-codex', 'antigravity', 'custom', 'opencode-go')),
  account_id TEXT NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  enabled INTEGER NOT NULL DEFAULT 1,
  show_usage INTEGER NOT NULL DEFAULT 1,
  show_third_party_usage INTEGER NOT NULL DEFAULT 0,
  usage_alert_window TEXT CHECK (usage_alert_window IS NULL OR usage_alert_window IN ('five_hour', 'weekly')),
  usage_alert_threshold INTEGER CHECK (usage_alert_threshold IS NULL OR usage_alert_threshold BETWEEN 1 AND 100)
);
INSERT INTO provider_accounts_next SELECT alias, provider_kind, account_id, created_at, enabled, show_usage, show_third_party_usage, usage_alert_window, usage_alert_threshold FROM provider_accounts;
DROP TABLE provider_accounts;
ALTER TABLE provider_accounts_next RENAME TO provider_accounts;
CREATE UNIQUE INDEX provider_accounts_account_id_unique ON provider_accounts(account_id);
INSERT INTO custom_provider_configs SELECT * FROM saved_go_custom;
INSERT INTO provider_usage_alert_deliveries SELECT * FROM saved_go_alerts;
INSERT INTO provider_model_exclusions SELECT * FROM saved_go_models;
INSERT INTO provider_transport_preferences SELECT * FROM saved_go_transport;
DROP TABLE saved_go_custom;
DROP TABLE saved_go_alerts;
DROP TABLE saved_go_models;
DROP TABLE saved_go_transport;
CREATE TABLE opencode_go_catalogs (
  alias TEXT PRIMARY KEY NOT NULL REFERENCES provider_accounts(alias) ON DELETE CASCADE,
  catalog TEXT NOT NULL
);
