CREATE TABLE `mcp_servers` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`kind` text NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`configured` integer DEFAULT false NOT NULL,
	`revision` integer DEFAULT 0 NOT NULL,
	`created_at` integer DEFAULT (unixepoch()) NOT NULL,
	CONSTRAINT "mcp_servers_kind" CHECK("mcp_servers"."kind" IN ('local', 'remote'))
);
--> statement-breakpoint
CREATE UNIQUE INDEX `mcp_servers_name_unique` ON `mcp_servers` (`name`);--> statement-breakpoint
ALTER TABLE `provider_accounts` ADD `enabled` integer DEFAULT true NOT NULL;
--> statement-breakpoint
INSERT INTO `mcp_servers` (`id`, `name`, `kind`, `enabled`, `configured`, `revision`) VALUES ('builtin-context7', 'context7', 'local', 1, 0, 0);
