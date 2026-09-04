CREATE TABLE `app_config` (
	`id` integer PRIMARY KEY NOT NULL,
	`onboarding_completed` integer DEFAULT false NOT NULL,
	CONSTRAINT "app_config_singleton_check" CHECK("app_config"."id" = 1)
);
