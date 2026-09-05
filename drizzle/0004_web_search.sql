CREATE TABLE `web_search_config` (
	`id` integer PRIMARY KEY NOT NULL,
	`account_alias` text,
	FOREIGN KEY (`account_alias`) REFERENCES `provider_accounts`(`alias`) ON UPDATE no action ON DELETE set null,
	CONSTRAINT "web_search_config_singleton" CHECK("web_search_config"."id" = 1)
);
