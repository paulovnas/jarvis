import { check, integer, sqliteTable } from "drizzle-orm/sqlite-core";
import { sql } from "drizzle-orm";

export const appConfig = sqliteTable(
  "app_config",
  {
    id: integer("id").primaryKey(),
    onboardingCompleted: integer("onboarding_completed", {
      mode: "boolean",
    })
      .notNull()
      .default(false),
  },
  (table) => [check("app_config_singleton_check", sql`${table.id} = 1`)],
);
