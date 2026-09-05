CREATE TEMP TABLE `saved_web_search` AS SELECT * FROM `web_search_config`;
--> statement-breakpoint
CREATE TABLE `new_provider_accounts` (
  `alias` text PRIMARY KEY NOT NULL,
  `provider_kind` text NOT NULL,
  `account_id` text NOT NULL,
  `created_at` integer DEFAULT (unixepoch()) NOT NULL,
  `enabled` integer DEFAULT true NOT NULL,
  CONSTRAINT "provider_accounts_provider_kind_check" CHECK("provider_kind" IN ('openai-codex', 'antigravity'))
);
--> statement-breakpoint
INSERT INTO `new_provider_accounts` SELECT `alias`, `provider_kind`, `account_id`, `created_at`, `enabled` FROM `provider_accounts`;
--> statement-breakpoint
DROP TABLE `provider_accounts`;
--> statement-breakpoint
ALTER TABLE `new_provider_accounts` RENAME TO `provider_accounts`;
--> statement-breakpoint
CREATE UNIQUE INDEX `provider_accounts_account_id_unique` ON `provider_accounts` (`account_id`);
--> statement-breakpoint
UPDATE `web_search_config` SET `account_alias` = (SELECT `account_alias` FROM `saved_web_search` WHERE `saved_web_search`.`id` = `web_search_config`.`id`);
--> statement-breakpoint
DROP TABLE `saved_web_search`;
