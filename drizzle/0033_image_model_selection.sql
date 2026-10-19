ALTER TABLE image_generation_config ADD COLUMN model TEXT;
UPDATE image_generation_config SET model = 'gemini-3.1-flash-image' WHERE account_alias IS NOT NULL;
