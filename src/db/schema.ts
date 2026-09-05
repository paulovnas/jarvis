import { check, index, integer, sqliteTable, text } from "drizzle-orm/sqlite-core";
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

export const providerAccounts = sqliteTable(
  "provider_accounts",
  {
    alias: text("alias").primaryKey(),
    providerKind: text("provider_kind").notNull(),
    accountId: text("account_id").notNull().unique(),
    createdAt: integer("created_at").notNull().default(sql`(unixepoch())`),
  },
  (table) => [
    check(
      "provider_accounts_provider_kind_check",
      sql`${table.providerKind} = 'openai-codex'`,
    ),
  ],
);

export const workspaces = sqliteTable("workspaces", {
  id: text("id").primaryKey(),
  name: text("name").notNull().unique(),
  createdAt: integer("created_at").notNull().default(sql`(unixepoch())`),
});

export const projects = sqliteTable("projects", {
  id: text("id").primaryKey(),
  workspaceId: text("workspace_id").notNull().references(() => workspaces.id),
  name: text("name").notNull(),
  path: text("path").notNull().unique(),
  createdAt: integer("created_at").notNull().default(sql`(unixepoch())`),
}, (table) => [index("projects_workspace_idx").on(table.workspaceId)]);

export const conversations = sqliteTable("conversations", {
  id: text("id").primaryKey(),
  projectId: text("project_id").notNull().references(() => projects.id),
  title: text("title").notNull(),
  displayTitle: text("display_title"),
  titleSource: text("title_source").notNull().default("manual"),
  createdAt: integer("created_at").notNull().default(sql`(unixepoch())`),
}, (table) => [
  index("conversations_project_idx").on(table.projectId),
  check("conversations_title_source", sql`${table.titleSource} IN ('default', 'manual', 'generated')`),
]);

export const navigationSelection = sqliteTable("navigation_selection", {
  id: integer("id").primaryKey(),
  workspaceId: text("workspace_id").references(() => workspaces.id),
  projectId: text("project_id").references(() => projects.id),
  conversationId: text("conversation_id").references(() => conversations.id),
}, (table) => [
  check("navigation_selection_singleton", sql`${table.id} = 1`),
  check("navigation_selection_hierarchy", sql`(${table.workspaceId} IS NOT NULL OR (${table.projectId} IS NULL AND ${table.conversationId} IS NULL)) AND (${table.projectId} IS NOT NULL OR ${table.conversationId} IS NULL)`),
]);
