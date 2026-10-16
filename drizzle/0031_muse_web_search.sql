ALTER TABLE web_search_config ADD COLUMN executor TEXT NOT NULL DEFAULT 'jarvis' CHECK (executor IN ('jarvis', 'muse'));
