# 06: MCP viewer — discover servers, probe tools on demand

**What to build:** A user sees every MCP server their harnesses would use, where each definition comes
from, and — when they ask — what tools it actually exposes and whether it connects. Nothing is spawned
behind their back.

**Context pointers:** `architecture.md` §Subsystems (mcp). Config files belong to other tools; read them
defensively and never rewrite them.

**Blocked by:** 01 (foundation shell)

**Status:** ready-for-agent

- [ ] Discovery reads `.mcp.json`, `~/.claude.json` (`mcpServers`), `~/.codex/config.toml`
      (`[mcp_servers]`), `opencode.json`, and adapter config, normalising to
      `{name, source, transport: stdio|http, command+args | url}`
- [ ] Duplicate definitions are deduped with all sources listed; a malformed config file produces a
      warning row, not a failure
- [ ] Probing is explicit only: stdio spawn → `initialize` + `tools/list` with a timeout; HTTP →
      `tools/list`; result carries state, tools (name + description), and a stderr tail on failure
- [ ] No probe runs on discovery, app start, or panel mount — asserted by test
- [ ] Probe kills its child on timeout and on drop (no orphans), tested with a fake sleeping server
- [ ] Tests use tempdir configs and fake stdio/HTTP servers; contract fixtures for `McpServerView`,
      `ServerProbe`
- [ ] MCP panel: servers grouped by source, state badge, Connect button per row, tool list, error detail
