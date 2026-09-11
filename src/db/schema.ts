import { check, index, integer, primaryKey, sqliteTable, text } from "drizzle-orm/sqlite-core";
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
    enabled: integer("enabled", { mode: "boolean" }).notNull().default(true),
    showUsage: integer("show_usage", { mode: "boolean" }).notNull().default(true),
    showThirdPartyUsage: integer("show_third_party_usage", { mode: "boolean" }).notNull().default(false),
    usageAlertWindow: text("usage_alert_window"),
    usageAlertThreshold: integer("usage_alert_threshold"),
    createdAt: integer("created_at").notNull().default(sql`(unixepoch())`),
  },
  (table) => [
    check(
      "provider_accounts_provider_kind_check",
      sql`${table.providerKind} IN ('openai-codex', 'antigravity', 'custom')`,
    ),
    check(
      "provider_accounts_usage_alert_window_check",
      sql`${table.usageAlertWindow} IS NULL OR ${table.usageAlertWindow} IN ('five_hour', 'weekly')`,
    ),
    check(
      "provider_accounts_usage_alert_threshold_check",
      sql`${table.usageAlertThreshold} IS NULL OR ${table.usageAlertThreshold} BETWEEN 1 AND 100`,
    ),
  ],
);

export const customProviderConfigs = sqliteTable("custom_provider_configs", {
  alias: text("alias").primaryKey().references(() => providerAccounts.alias, { onDelete: "cascade" }),
  config: text("config").notNull(),
});

export const providerUsageAlertDeliveries = sqliteTable("provider_usage_alert_deliveries", {
  alias: text("alias").notNull().references(() => providerAccounts.alias, { onDelete: "cascade" }),
  windowId: text("window_id").notNull(),
  resetsAt: integer("resets_at").notNull(),
  threshold: integer("threshold").notNull(),
}, (table) => [primaryKey({ columns: [table.alias, table.windowId] })]);

export const mcpServers = sqliteTable("mcp_servers", {
  id: text("id").primaryKey(),
  name: text("name").notNull().unique(),
  kind: text("kind").notNull(),
  enabled: integer("enabled", { mode: "boolean" }).notNull().default(true),
  configured: integer("configured", { mode: "boolean" }).notNull().default(false),
  revision: integer("revision").notNull().default(0),
  lastCheck: text("last_check"),
  createdAt: integer("created_at").notNull().default(sql`(unixepoch())`),
}, (table) => [check("mcp_servers_kind", sql`${table.kind} IN ('local', 'remote')`)]);

export const webSearchConfig = sqliteTable("web_search_config", {
  id: integer("id").primaryKey(),
  inheritChat: integer("inherit_chat", { mode: "boolean" }).notNull().default(true),
  model: text("model").default("gpt-5.6-luna"),
  accountAlias: text("account_alias"),
}, (table) => [check("web_search_config_singleton", sql`${table.id} = 1`)]);

export const visionConfig = sqliteTable("vision_config", {
  id: integer("id").primaryKey(),
  inheritChat: integer("inherit_chat", { mode: "boolean" }).notNull().default(true),
  accountAlias: text("account_alias"),
  model: text("model"),
}, (table) => [check("vision_config_singleton", sql`${table.id} = 1`)]);

export const imageGenerationConfig = sqliteTable("image_generation_config", {
  id: integer("id").primaryKey(),
  accountAlias: text("account_alias"),
}, (table) => [check("image_generation_config_singleton", sql`${table.id} = 1`)]);

export const providerModelBindings = sqliteTable("provider_model_bindings", {
  itemKey: text("item_key").notNull(),
  source: text("source").notNull(),
  target: text("target").notNull(),
}, (table) => [primaryKey({ columns: [table.itemKey, table.source] }), check("provider_binding_source_json", sql`json_valid(${table.source})`), check("provider_binding_target_json", sql`json_valid(${table.target})`)]);

export const providerBindingsRevision = sqliteTable("provider_bindings_revision", {
  id: integer("id").primaryKey(),
  revision: integer("revision").notNull().default(0),
}, (table) => [check("provider_bindings_singleton", sql`${table.id} = 1`), check("provider_bindings_revision_positive", sql`${table.revision} >= 0`)]);

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

export const projectPublicationSettings = sqliteTable("project_publication_settings", {
  projectId: text("project_id").primaryKey().references(() => projects.id, { onDelete: "cascade" }),
  publishPrompt: text("publish_prompt").notNull(),
  prMode: text("pr_mode", { enum: ["disabled", "ask_pr", "ask_pr_merge"] }).notNull().default("disabled"),
  prPrompt: text("pr_prompt").notNull(),
  updatedAt: integer("updated_at").notNull().default(sql`(unixepoch())`),
}, (table) => [check("project_publication_pr_mode", sql`${table.prMode} IN ('disabled', 'ask_pr', 'ask_pr_merge')`)]);

export const conversations = sqliteTable("conversations", {
  id: text("id").primaryKey(),
  projectId: text("project_id").notNull().references(() => projects.id),
  title: text("title").notNull(),
  displayTitle: text("display_title"),
  titleSource: text("title_source").notNull().default("manual"),
  lastActivityAt: integer("last_activity_at"),
  createdAt: integer("created_at").notNull().default(sql`(unixepoch())`),
}, (table) => [
  index("conversations_project_idx").on(table.projectId),
  check("conversations_title_source", sql`${table.titleSource} IN ('default', 'manual', 'generated')`),
]);

export const conversationUnread = sqliteTable("conversation_unread", {
  conversationId: text("conversation_id").primaryKey().references(() => conversations.id, { onDelete: "cascade" }),
  eventKey: text("event_key").notNull(),
  unread: integer("unread", { mode: "boolean" }).notNull().default(true),
}, (table) => [check("conversation_unread_flag", sql`${table.unread} IN (0, 1)`)]);

export const navigationSelection = sqliteTable("navigation_selection", {
  id: integer("id").primaryKey(),
  workspaceId: text("workspace_id").references(() => workspaces.id),
  projectId: text("project_id").references(() => projects.id),
  conversationId: text("conversation_id").references(() => conversations.id),
}, (table) => [
  check("navigation_selection_singleton", sql`${table.id} = 1`),
  check("navigation_selection_hierarchy", sql`(${table.workspaceId} IS NOT NULL OR (${table.projectId} IS NULL AND ${table.conversationId} IS NULL)) AND (${table.projectId} IS NOT NULL OR ${table.conversationId} IS NULL)`),
]);
