-- Use additive columns: rebuilding a referenced table would break persisted navigation.
ALTER TABLE `conversations` ADD `display_title` text;
--> statement-breakpoint
ALTER TABLE `conversations` ADD `title_source` text DEFAULT 'manual' NOT NULL
  CONSTRAINT "conversations_title_source" CHECK("title_source" IN ('default', 'manual', 'generated'));
