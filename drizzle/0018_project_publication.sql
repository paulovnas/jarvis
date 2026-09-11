CREATE TABLE `project_publication_settings` (
	`project_id` text PRIMARY KEY NOT NULL,
	`publish_prompt` text NOT NULL,
	`pr_mode` text DEFAULT 'disabled' NOT NULL,
	`pr_prompt` text NOT NULL,
	`updated_at` integer DEFAULT (unixepoch()) NOT NULL,
	FOREIGN KEY (`project_id`) REFERENCES `projects`(`id`) ON UPDATE no action ON DELETE cascade,
	CONSTRAINT "project_publication_pr_mode" CHECK(`pr_mode` IN ('disabled', 'ask_pr', 'ask_pr_merge'))
);
