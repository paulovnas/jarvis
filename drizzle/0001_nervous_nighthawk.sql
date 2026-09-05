CREATE TABLE `provider_accounts` (
	`alias` text PRIMARY KEY NOT NULL,
	`provider_kind` text NOT NULL,
	`account_id` text NOT NULL,
	`created_at` integer DEFAULT (unixepoch()) NOT NULL,
	CONSTRAINT "provider_accounts_provider_kind_check" CHECK("provider_accounts"."provider_kind" = 'openai-codex')
);
--> statement-breakpoint
CREATE UNIQUE INDEX `provider_accounts_account_id_unique` ON `provider_accounts` (`account_id`);