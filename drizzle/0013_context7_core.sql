-- Core owns Context7 now. Preserve any manually configured MCP and its credentials.
DELETE FROM mcp_servers WHERE id = 'builtin-context7' AND configured = 0 AND revision = 0;
