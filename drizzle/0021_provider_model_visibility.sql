CREATE TABLE provider_model_exclusions (
  account_alias TEXT NOT NULL REFERENCES provider_accounts(alias) ON DELETE CASCADE,
  model_id TEXT NOT NULL,
  PRIMARY KEY (account_alias, model_id)
);
