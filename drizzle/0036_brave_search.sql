-- Preserve cascading provider preferences while adding the search-only provider.
CREATE TEMP TABLE saved_brave_custom AS SELECT * FROM custom_provider_configs;
CREATE TEMP TABLE saved_brave_alerts AS SELECT * FROM provider_usage_alert_deliveries;
CREATE TEMP TABLE saved_brave_models AS SELECT * FROM provider_model_exclusions;
CREATE TEMP TABLE saved_brave_transport AS SELECT * FROM provider_transport_preferences;
CREATE TEMP TABLE saved_brave_go_catalogs AS SELECT * FROM opencode_go_catalogs;
CREATE TABLE provider_accounts_next (
  alias TEXT PRIMARY KEY NOT NULL,
  provider_kind TEXT NOT NULL CHECK (provider_kind IN ('openai-codex', 'antigravity', 'custom', 'opencode-go', 'charm-hyper', 'openrouter', 'ollama', 'brave-search')),
  account_id TEXT NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  enabled INTEGER NOT NULL DEFAULT 1,
  show_usage INTEGER NOT NULL DEFAULT 1,
  show_third_party_usage INTEGER NOT NULL DEFAULT 0,
  usage_alert_window TEXT CHECK (usage_alert_window IS NULL OR usage_alert_window IN ('five_hour', 'weekly')),
  usage_alert_threshold INTEGER CHECK (usage_alert_threshold IS NULL OR usage_alert_threshold BETWEEN 1 AND 100),
  show_five_hour_usage INTEGER NOT NULL DEFAULT 1 CHECK (show_five_hour_usage IN (0, 1)),
  show_weekly_usage INTEGER NOT NULL DEFAULT 1 CHECK (show_weekly_usage IN (0, 1)),
  display_name TEXT CHECK (display_name IS NULL OR length(trim(display_name)) BETWEEN 1 AND 18)
);
INSERT INTO provider_accounts_next SELECT * FROM provider_accounts;
DROP TABLE provider_accounts;
ALTER TABLE provider_accounts_next RENAME TO provider_accounts;
CREATE UNIQUE INDEX provider_accounts_account_id_unique ON provider_accounts(account_id);
INSERT INTO custom_provider_configs SELECT * FROM saved_brave_custom;
INSERT INTO provider_usage_alert_deliveries SELECT * FROM saved_brave_alerts;
INSERT INTO provider_model_exclusions SELECT * FROM saved_brave_models;
INSERT INTO provider_transport_preferences SELECT * FROM saved_brave_transport;
INSERT INTO opencode_go_catalogs SELECT * FROM saved_brave_go_catalogs;
DROP TABLE saved_brave_custom;
DROP TABLE saved_brave_alerts;
DROP TABLE saved_brave_models;
DROP TABLE saved_brave_transport;
DROP TABLE saved_brave_go_catalogs;
