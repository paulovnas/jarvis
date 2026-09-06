CREATE TABLE `vision_config` (
	`id` integer PRIMARY KEY NOT NULL,
	`account_alias` text,
	`model` text,
	FOREIGN KEY (`account_alias`) REFERENCES `provider_accounts`(`alias`) ON UPDATE no action ON DELETE set null,
	CONSTRAINT "vision_config_singleton" CHECK("vision_config"."id" = 1)
);
--> statement-breakpoint
ALTER TABLE `web_search_config` ADD `model` text DEFAULT 'gpt-5.6-luna';