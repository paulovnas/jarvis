CREATE TABLE image_generation_config (
  id INTEGER PRIMARY KEY NOT NULL,
  account_alias TEXT REFERENCES provider_accounts(alias) ON DELETE SET NULL,
  CONSTRAINT image_generation_config_singleton CHECK (id = 1)
);
