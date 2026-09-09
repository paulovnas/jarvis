ALTER TABLE provider_accounts ADD COLUMN usage_alert_window TEXT
  CHECK (usage_alert_window IS NULL OR usage_alert_window IN ('five_hour', 'weekly'));
ALTER TABLE provider_accounts ADD COLUMN usage_alert_threshold INTEGER
  CHECK (usage_alert_threshold IS NULL OR usage_alert_threshold BETWEEN 1 AND 100);

CREATE TABLE provider_usage_alert_deliveries (
  alias TEXT NOT NULL REFERENCES provider_accounts(alias) ON DELETE CASCADE,
  window_id TEXT NOT NULL,
  resets_at INTEGER NOT NULL,
  threshold INTEGER NOT NULL,
  PRIMARY KEY (alias, window_id)
);
