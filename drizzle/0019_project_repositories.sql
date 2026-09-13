CREATE TABLE `project_repositories` (
	`id` text PRIMARY KEY NOT NULL,
	`project_id` text NOT NULL,
	`path` text NOT NULL,
	`name` text NOT NULL,
	`description` text NOT NULL DEFAULT '',
	`created_at` integer DEFAULT (unixepoch()) NOT NULL,
	`updated_at` integer DEFAULT (unixepoch()) NOT NULL,
	FOREIGN KEY (`project_id`) REFERENCES `projects`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `project_repositories_project_path_unique` ON `project_repositories` (`project_id`,`path`);
--> statement-breakpoint
CREATE INDEX `project_repositories_project_idx` ON `project_repositories` (`project_id`);
