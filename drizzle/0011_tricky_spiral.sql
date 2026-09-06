ALTER TABLE `vision_config` ADD `inherit_chat` integer DEFAULT true NOT NULL;--> statement-breakpoint
ALTER TABLE `web_search_config` ADD `inherit_chat` integer DEFAULT true NOT NULL;

--> statement-breakpoint
UPDATE web_search_config SET inherit_chat = 0 WHERE account_alias IS NOT NULL;
--> statement-breakpoint
UPDATE vision_config SET inherit_chat = 0 WHERE account_alias IS NOT NULL;
