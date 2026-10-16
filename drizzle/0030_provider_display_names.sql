ALTER TABLE provider_accounts ADD COLUMN display_name TEXT
  CHECK (display_name IS NULL OR length(trim(display_name)) BETWEEN 1 AND 18);
