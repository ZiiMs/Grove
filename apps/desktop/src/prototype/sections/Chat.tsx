/**
 * PROTOTYPE — throwaway. Chat & agent specimens: turns, tool cards, diffs, approvals, composer.
 */

const diffRows: { kind: "ctx" | "add" | "del" | "hunk"; ln: number; text: string }[] = [
  { kind: "hunk", ln: 0, text: "@@ -12,7 +12,9 @@ impl HarnessAdapter for AcpAdapter" },
  { kind: "ctx", ln: 12, text: "    fn id(&self) -> &'static str {" },
  { kind: "del", ln: 13, text: '        "acp"' },
  { kind: "add", ln: 13, text: '        "acp"' },
  { kind: "add", ln: 14, text: "    }" },
  { kind: "add", ln: 15, text: "" },
  { kind: "add", ln: 16, text: "    fn supports_resume(&self) -> bool {" },
  { kind: "add", ln: 17, text: "        true" },
  { kind: "ctx", ln: 18, text: "    }" },
];

export function ChatSection() {
  return (
    <div className="pv-grid-2">
      <div className="pv-block">
        <div className="pv-block-head">
          <h2>Example thread</h2>
          <span>Claude Code session, streaming turn</span>
        </div>
        <div className="pv-card">
          <div className="pv-card-body">
            <div className="pv-thread">
              <div className="pv-turn is-user">
                <span className="pv-avatar">ZY</span>
                <div className="pv-turn-main">
                  <div className="pv-turn-name">you · 14:02</div>
                  <div className="pv-bubble">
                    Add the ACP adapter behind <span className="pv-code">HarnessAdapter</span> and keep the
                    other four untouched.
                  </div>
                </div>
              </div>

              <div className="pv-turn is-agent">
                <span className="pv-avatar">CC</span>
                <div className="pv-turn-main">
                  <div className="pv-turn-name">
                    Claude Code <span className="pv-badge is-accent">opus</span> · 14:02
                  </div>
                  <div className="pv-thinking">
                    Spec 03 pins the adapter seam: one trait, no cross-adapter imports. Checking the existing
                    five…
                  </div>
                  <div className="pv-bubble">
                    The seam already fits — ACP needs <span className="pv-code">supports_resume</span> and a
                    stdio transport. Writing the adapter now.<span className="pv-stream-caret" />
                  </div>

                  <div className="pv-tool">
                    <div className="pv-tool-head">
                      <span className="pv-badge is-accent">read</span>
                      <b>crates/grove-core/src/adapters/mod.rs</b>
                      <span className="pv-rail-spacer" />
                      <span className="pv-faint">1.2k bytes · 42ms</span>
                    </div>
                    <div className="pv-tool-body">
                      {`pub trait HarnessAdapter: Send + Sync {\n    fn id(&self) -> &'static str;\n    fn spawn(&self, cfg: SessionConfig) -> Result<SessionHandle>;\n}`}
                    </div>
                  </div>

                  <div className="pv-diff">
                    <div className="pv-diff-head">
                      <span className="pv-badge">edit</span>
                      <b className="pv-mono">crates/grove-core/src/adapters/acp.rs</b>
                      <span className="pv-rail-spacer" />
                      <span style={{ color: "var(--success)" }}>+7</span>
                      <span style={{ color: "var(--danger)" }}>−1</span>
                    </div>
                    {diffRows.map((row) => (
                      <div key={`${row.kind}-${row.ln}-${row.text}`} className={`pv-diff-row is-${row.kind}`}>
                        <span className="ln">{row.ln || ""}</span>
                        <span className="tx">
                          {row.kind === "add" ? "+" : row.kind === "del" ? "−" : " "}
                          {row.text}
                        </span>
                      </div>
                    ))}
                  </div>

                  <div className="pv-approval">
                    <div className="pv-approval-head">
                      <span className="pv-status-dot is-warn" /> Thinks it needs your say-so
                    </div>
                    <div className="pv-small pv-muted">
                      Run <span className="pv-mono">cargo test -p grove-core -- adapter</span> — 34 tests.
                    </div>
                    <div className="pv-row">
                      <button type="button" className="pv-btn sz-sm is-primary">
                        Allow once
                      </button>
                      <button type="button" className="pv-btn sz-sm">
                        Always allow cargo test
                      </button>
                      <button type="button" className="pv-btn sz-sm is-ghost">
                        Deny
                      </button>
                    </div>
                  </div>
                </div>
              </div>

              <div className="pv-turn is-agent">
                <span className="pv-avatar">CX</span>
                <div className="pv-turn-main">
                  <div className="pv-turn-name">Codex · 14:05</div>
                  <div className="pv-errorstate">
                    <span style={{ color: "var(--danger)" }}>✕</span>
                    <div>
                      <div style={{ fontWeight: 600 }}>Session exited with code 2</div>
                      <div className="pv-faint pv-small">
                        app-server handshake timed out after 15s — the harness may not be installed.
                      </div>
                    </div>
                    <span className="pv-rail-spacer" style={{ flex: 1 }} />
                    <button type="button" className="pv-btn sz-sm">
                      Retry
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="pv-col" style={{ gap: "var(--pad-x)" }}>
        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Composer</h2>
            <span>attachments, skill picker, model, send</span>
          </div>
          <div className="pv-card">
            <div className="pv-card-body">
              <div className="pv-composer">
                <textarea
                  className="pv-composer-input"
                  placeholder="Ask Claude Code to do something…"
                  defaultValue=""
                />
                <div className="pv-composer-tools">
                  <button type="button" className="pv-btn sz-sm is-ghost">
                    ⊕ Attach
                  </button>
                  <button type="button" className="pv-btn sz-sm is-ghost">
                    ✦ Skill
                  </button>
                  <button type="button" className="pv-btn sz-sm is-ghost">
                    ◈ Spec
                  </button>
                  <span className="pv-rail-spacer" />
                  <span className="pv-model-pick">claude-opus-4 ▾</span>
                  <button type="button" className="pv-btn sz-sm is-primary">
                    Send ↵
                  </button>
                </div>
              </div>
              <div className="pv-row">
                <span className="pv-chip">
                  code-review <span className="pv-x">✕</span>
                </span>
                <span className="pv-chip">
                  spec/03-adapter-interface <span className="pv-x">✕</span>
                </span>
              </div>
            </div>
          </div>
        </div>

        <div className="pv-block">
          <div className="pv-block-head">
            <h2>Turn states</h2>
            <span>queued, thinking, compact, collapsed tool</span>
          </div>
          <div className="pv-card">
            <div className="pv-card-body">
              <div className="pv-row pv-small pv-faint">
                <span className="pv-status-dot" /> Queued — sends when the current turn ends
              </div>
              <div className="pv-thinking">Thinking · 8s · reading 3 files</div>
              <div className="pv-tool">
                <div className="pv-tool-head">
                  <span className="pv-badge">grep</span>
                  <b>supports_resume</b>
                  <span className="pv-rail-spacer" />
                  <span className="pv-faint">▸ 4 results</span>
                </div>
              </div>
              <div className="pv-tool">
                <div className="pv-tool-head">
                  <span className="pv-badge is-success">test</span>
                  <b>cargo test</b>
                  <span className="pv-badge is-success">34 pass</span>
                  <span className="pv-rail-spacer" />
                  <span className="pv-faint">2.1s</span>
                </div>
              </div>
              <div className="pv-row">
                <button type="button" className="pv-btn sz-sm is-ghost">
                  ⏸ Pause
                </button>
                <button type="button" className="pv-btn sz-sm is-ghost">
                  ⏹ Stop
                </button>
                <button type="button" className="pv-btn sz-sm is-ghost">
                  ⧉ Fork session
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
