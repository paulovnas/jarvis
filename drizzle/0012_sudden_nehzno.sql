-- Native migrations run inside a transaction with foreign keys enabled.
-- Preserve tool selections across the parent table rebuild.
CREATE TEMP TABLE saved_custom_web_search AS SELECT * FROM web_search_config;
CREATE TEMP TABLE saved_custom_vision AS SELECT * FROM vision_config;
CREATE TABLE `__new_provider_accounts` (
	`alias` text PRIMARY KEY NOT NULL,
	`provider_kind` text NOT NULL,
	`account_id` text NOT NULL,
	`created_at` integer DEFAULT (unixepoch()) NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`show_usage` integer DEFAULT true NOT NULL,
	`show_third_party_usage` integer DEFAULT false NOT NULL,
	CONSTRAINT "provider_accounts_provider_kind_check" CHECK("__new_provider_accounts"."provider_kind" IN ('openai-codex', 'antigravity', 'custom'))
);
--> statement-breakpoint
INSERT INTO `__new_provider_accounts`("alias", "provider_kind", "account_id", "enabled", "show_usage", "show_third_party_usage", "created_at") SELECT "alias", "provider_kind", "account_id", "enabled", "show_usage", "show_third_party_usage", "created_at" FROM `provider_accounts`;--> statement-breakpoint
DROP TABLE `provider_accounts`;--> statement-breakpoint
ALTER TABLE `__new_provider_accounts` RENAME TO `provider_accounts`;--> statement-breakpoint
CREATE UNIQUE INDEX `provider_accounts_account_id_unique` ON `provider_accounts` (`account_id`);
UPDATE web_search_config SET account_alias = (SELECT account_alias FROM saved_custom_web_search WHERE saved_custom_web_search.id = web_search_config.id);
UPDATE vision_config SET account_alias = (SELECT account_alias FROM saved_custom_vision WHERE saved_custom_vision.id = vision_config.id);
DROP TABLE saved_custom_web_search;
DROP TABLE saved_custom_vision;
CREATE TABLE `custom_provider_configs` (
  `alias` text PRIMARY KEY NOT NULL,
  `config` text NOT NULL,
  FOREIGN KEY (`alias`) REFERENCES `provider_accounts`(`alias`) ON UPDATE no action ON DELETE cascade
);
