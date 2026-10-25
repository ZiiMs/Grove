/**
 * PROTOTYPE — throwaway. Data & status specimens: spec tree, skill grid, todo, MCP, git, states.
 */

const specTree = [
  { depth: 0, name: "goal-and-requirements.md", meta: "goal", active: false, edge: "" },
  { depth: 0, name: "architecture.md", meta: "arch", active: true, edge: "" },
  { depth: 1, name: "03-adapter-interface", meta: "module", active: false, edge: "└─ depends-on" },
  { depth: 1, name: "04-ipc-contract", meta: "module", active: false, edge: "└─ implements" },
  { depth: 2, name: "contract-fixtures", meta: "sub", active: false, edge: "   └─ references" },
  { depth: 1, name: "05-session-persistence", meta: "module", active: false, edge: "└─ depends-on" },
];

const skills = [
  { name: "code-review", src: "mattpocock/skills", tag: "review", installed: true },
  { name: "brainstorming", src: "anthropics/skills", tag: "plan", installed: true },
  { name: "tdd", src: "mattpocock/skills", tag: "build", installed: false },
  { name: "diagnosing-bugs", src: "anthropics/skills", tag: "debug", installed: false },
  { name: "spec-graph", src: "grove/skills", tag: "specs", installed: true },
  { name: "shipping-a-pr", src: "mattpocock/skills", tag: "ship", installed: false },
];

const todos = [
  { text: "Pin spec 03 to the ACP session", done: true },
  { text: "Add the adapter behind HarnessAdapter", done: true },
  { text: "Contract fixture for the session DTO", done: false, active: true },
  { text: "Wire resume through the sidecar", done: false },
  { text: "Diff card + approval flow", done: false },
];

const mcp = [
  { name: "filesystem", transport: "stdio", tools: 11, state: "ok" },
  { name: "github", transport: "http", tools: 24, state: "ok" },
  { name: "linear", transport: "http", tools: 8, state: "warn" },
  { name: "sqlite", transport: "stdio", tools: 6, state: "idle" },
  { name: "sentry", transport: "sse", tools: 0, state: "err" },
];

const gitRows = [
  { branch: "grove-project-setup", meta: "ahead 3", state: "ok", extra: "#12 draft" },
  { branch: "feat/acp-adapter", meta: "ahead 1", state: "live", extra: "#14 checks" },
  { branch: "fix/contract-zod", meta: "behind 2", state: "warn", extra: "#11 review" },
  { branch: "main", meta: "clean", state: "ok", extra: "—" },
];

function dotClass(state: string) {
  return `pv-status-dot is-${state}`;
}

export function DataSection() {
  return (
    <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
      <div className="pv-grid-2">
        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Spec graph</h2>
            <span>tree rows carry the edge kind, not just nesting</span>
          </div>
          <div className="pv-card">
            <div className="pv-card-head">
              <b>grove</b> · 12 specs
              <span className="pv-rail-spacer" />
              <button type="button" className="pv-btn sz-sm is-ghost">
                + New spec
              </button>
            </div>
            <div className="pv-tree" style={{ padding: 8 }}>
              {specTree.map((node) => (
                <div
                  key={node.name}
                  className={node.active ? "pv-tree-row is-active" : "pv-tree-row"}
                  style={{ paddingLeft: 8 + node.depth * 14 }}
                >
                  <span className="pv-faint">{node.meta === "arch" ? "◈" : "◇"}</span>
                  <span className="pv-tree-name">{node.name}</span>
                  {node.edge ? <span className="pv-edge">{node.edge}</span> : null}
                  <span className="pv-badge">{node.meta}</span>
                </div>
              ))}
            </div>
          </div>
        </div>

        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Skill catalog</h2>
            <span>installed badge + source repo</span>
          </div>
          <div className="pv-card">
            <div className="pv-card-head">
              <div className="pv-input-wrap" style={{ height: 26, flex: 1 }}>
                <span className="pv-faint">⌕</span>
                <input className="pv-input" placeholder="Search 240 skills…" />
              </div>
            </div>
            <div className="pv-card-body">
              <div className="pv-skill-grid">
                {skills.map((skill) => (
                  <div key={skill.name} className="pv-skill-card">
                    <div className="pv-skill-name">
                      <span style={{ color: "var(--accent)" }}>✦</span>
                      {skill.name}
                    </div>
                    <div className="pv-faint pv-small pv-mono">{skill.src}</div>
                    <div className="pv-row">
                      <span className="pv-badge">{skill.tag}</span>
                      {skill.installed ? (
                        <span className="pv-badge is-success">installed</span>
                      ) : (
                        <button type="button" className="pv-btn sz-sm">
                          Install
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="pv-grid-2">
        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Todo plan</h2>
            <span>2 of 5 · survives restarts</span>
          </div>
          <div className="pv-card">
            <div className="pv-card-body">
              <div className="pv-progress">
                <i style={{ width: "40%" }} />
              </div>
              <div className="pv-todo">
                {todos.map((todo) => (
                  <div
                    key={todo.text}
                    className={`pv-todo-row${todo.done ? " is-done" : ""}${todo.active ? " is-active" : ""}`}
                  >
                    <label className="pv-check">
                      <input type="checkbox" defaultChecked={todo.done} />
                    </label>
                    <span className="pv-todo-text">{todo.text}</span>
                    {todo.active ? <span className="pv-badge is-accent">now</span> : null}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>

        <div className="pv-block">
          <div className="pv-block-head">
            <h2>MCP servers</h2>
            <span>transport, tool count, live state</span>
          </div>
          <div className="pv-card">
            <div className="pv-table cols-mcp">
              <div className="pv-table-head">
                <span />
                <span>server</span>
                <span>transport</span>
                <span>tools</span>
                <span>state</span>
              </div>
              {mcp.map((server) => (
                <div key={server.name} className="pv-table-row">
                  <span className={dotClass(server.state)} />
                  <span className="pv-mono">{server.name}</span>
                  <span className="pv-small pv-faint">{server.transport}</span>
                  <span>{server.tools}</span>
                  <span className="pv-small pv-faint">{server.state}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      <div className="pv-block">
        <div className="pv-block-head">
          <h2>Branches & PRs</h2>
          <span>one row per worktree, checks inline</span>
        </div>
        <div className="pv-card">
          <div className="pv-table cols-git">
            <div className="pv-table-head">
              <span />
              <span>branch</span>
              <span>sync</span>
              <span>pr</span>
            </div>
            {gitRows.map((row) => (
              <div key={row.branch} className="pv-table-row">
                <span className={dotClass(row.state)} />
                <span className="pv-branch">⑂ {row.branch}</span>
                <span className="pv-small pv-faint">{row.meta}</span>
                <span className="pv-small">{row.extra}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="pv-block">
        <div className="pv-block-head">
          <h2>Empty, loading, error, toast</h2>
          <span>the states that get skipped and then hurt</span>
        </div>
        <div className="pv-grid-2">
          <div className="pv-empty">
            <span className="pv-empty-glyph">◇</span>
            <b className="pv-small">No specs yet</b>
            <span className="pv-small pv-faint">
              Draft a goal doc, or point Grove at an existing <span className="pv-code">SPEC.md</span>.
            </span>
            <button type="button" className="pv-btn sz-sm is-primary" style={{ marginTop: 4 }}>
              Create goal doc
            </button>
          </div>
          <div className="pv-card">
            <div className="pv-card-body">
              <div className="pv-skel" style={{ width: "62%" }} />
              <div className="pv-skel" style={{ width: "88%" }} />
              <div className="pv-skel" style={{ width: "44%" }} />
              <div className="pv-row" style={{ marginTop: 4 }}>
                <div className="pv-toast is-ok">
                  <span style={{ color: "var(--success)" }}>✓</span> code-review installed into{" "}
                  <span className="pv-mono">.claude/skills</span>
                </div>
              </div>
              <div className="pv-toast is-err">
                <span style={{ color: "var(--danger)" }}>✕</span> Could not reach GitHub — showing cached
                catalog
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
