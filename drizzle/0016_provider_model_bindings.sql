-- A removed account remains an explicit, invalid selection until the user fixes it.
-- SET NULL used to silently turn tools off and erased the information needed to explain why.
CREATE TABLE web_search_config_next (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  account_alias TEXT,
  model TEXT DEFAULT 'gpt-5.6-luna',
  inherit_chat INTEGER NOT NULL DEFAULT 1
);
INSERT INTO web_search_config_next SELECT id, account_alias, model, inherit_chat FROM web_search_config;
DROP TABLE web_search_config;
ALTER TABLE web_search_config_next RENAME TO web_search_config;
CREATE TABLE vision_config_next (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  account_alias TEXT,
  model TEXT,
  inherit_chat INTEGER NOT NULL DEFAULT 1
);
INSERT INTO vision_config_next SELECT id, account_alias, model, inherit_chat FROM vision_config;
DROP TABLE vision_config;
ALTER TABLE vision_config_next RENAME TO vision_config;
CREATE TABLE image_generation_config_next (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  account_alias TEXT
);
INSERT INTO image_generation_config_next SELECT id, account_alias FROM image_generation_config;
DROP TABLE image_generation_config;
ALTER TABLE image_generation_config_next RENAME TO image_generation_config;

-- Explicit replacements are guarded by the original choice. Editing a configuration
-- makes an old binding inapplicable; conversation journals remain immutable.
CREATE TABLE provider_model_bindings (
  item_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK (json_valid(source)),
  target TEXT NOT NULL CHECK (json_valid(target)),
  PRIMARY KEY (item_key, source)
);
CREATE TABLE provider_bindings_revision (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)
);
INSERT INTO provider_bindings_revision (id) VALUES (1);
