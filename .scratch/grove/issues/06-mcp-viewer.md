# 06: MCP viewer — discover servers, probe tools on demand

**What to build:** A user sees every MCP server their harnesses would use, where each definition comes
from, and — when they ask — what tools it actually exposes and whether it connects. Nothing is spawned
behind their back.

**Context pointers:** `architecture.md` §Subsystems (mcp). Config files belong to other tools; read them
defensively and never rewrite them.

**Blocked by:** 01 (foundation shell)

**Status:** done

- [x] Discovery reads `.mcp.json`, `~/.claude.json` (`mcpServers`), `~/.codex/config.toml`
      (`[mcp_servers]`), `opencode.json`, and adapter config, normalising to
      `{name, source, transport: stdio|http, command+args | url}`
- [x] Duplicate definitions are deduped with all sources listed; a malformed config file produces a
      warning row, not a failure
- [x] Probing is explicit only: stdio spawn → `initialize` + `tools/list` with a timeout; HTTP →
      `tools/list`; result carries state, tools (name + description), and a stderr tail on failure
- [x] No probe runs on discovery, app start, or panel mount — asserted by test
- [x] Probe kills its child on timeout and on drop (no orphans), tested with a fake sleeping server
- [x] Tests use tempdir configs and fake stdio/HTTP servers; contract fixtures for `McpServerView`,
      `ServerProbe`
- [x] MCP panel: servers grouped by source, state badge, Connect button per row, tool list, error detail

## Resolution

Merged as `feat(mcp): discover MCP servers and probe tools on demand`.

Decisions worth knowing:

- `mcp_servers` returns `McpDiscovery { servers, warnings }` rather than only servers: a malformed file has
  no transport or command, and folding it into `McpServerView` would make the type lie.
- Probing re-runs discovery and requires the requested source to still list the server, so a stale row can
  never spawn a process. A refused connection is a `ServerProbe { state: "error" }`, not a command error.
- The stdio probe uses `kill_on_drop` plus an explicit kill on timeout, proven by a fake server whose
  heartbeat file stops changing — no orphaned processes.
- Server `env` and HTTP `headers` are deliberately parsed and discarded, so a token in an MCP config can
  never reach the webview.
- Discovery never spawns anything (asserted with a startup-marker file), which is what keeps opening the
  panel side-effect free.

Not verified: no real third-party MCP server or real harness config was exercised; tests use synthetic
tempdir configs and fake stdio/HTTP servers.
