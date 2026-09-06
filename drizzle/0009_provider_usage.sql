ALTER TABLE `provider_accounts` ADD `show_usage` integer DEFAULT true NOT NULL;--> statement-breakpoint
ALTER TABLE `provider_accounts` ADD `show_third_party_usage` integer DEFAULT false NOT NULL;