ALTER TABLE provider_accounts ADD COLUMN show_five_hour_usage INTEGER NOT NULL DEFAULT 1 CHECK (show_five_hour_usage IN (0, 1));
--> statement-breakpoint
ALTER TABLE provider_accounts ADD COLUMN show_weekly_usage INTEGER NOT NULL DEFAULT 1 CHECK (show_weekly_usage IN (0, 1));
