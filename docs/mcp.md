# Model Context Protocol (MCP) Integration

Faultline implements the Model Context Protocol (MCP) over standard I/O (`stdio`), allowing AI coding assistants like Claude Code, Cursor, Gemini, and custom agents to invoke Faultline tools directly.

## Available MCP Tools

1. `get_project_status`: Returns project configuration and session overview.
2. `inspect_schema`: Returns discovered tables, types, and constraints.
3. `inspect_migration`: Analyzes migration SQL and suggests search strategies.
4. `list_strategies`: Lists available counterexample strategies.
5. `list_counterexamples`: Lists discovered migration counterexamples.

## Configuring MCP in Claude Desktop / Cursor

Add Faultline to your `claude_desktop_config.json` or Cursor settings:

```json
{
  "mcpServers": {
    "faultline": {
      "command": "faultline",
      "args": ["mcp"]
    }
  }
}
```

## Agent Philosophy
> "AI may propose. Faultline must prove."
