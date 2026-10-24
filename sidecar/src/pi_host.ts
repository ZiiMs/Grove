/**
 * The pi host: it drives `pi --mode rpc` and rewrites pi's records as normalized Grove events.
 *
 * pi's RPC mode is JSONL on stdin/stdout (`--mode rpc`), so the sidecar is a thin shape-rewriter, not
 * an SDK host. Two rules from pi's documentation are load-bearing here:
 *
 * - **Framing is LF-only.** Records are split on `\n` (an optional preceding CR is dropped) by
 *   [`createLineSplitter`], never by `readline` — which also splits on U+2028/U+2029, characters that
 *   are legal unescaped inside JSON strings and therefore inside prompts.
 * - **`agent_settled` is the idle signal, not `agent_end`.** pi may keep working after `agent_end`
 *   (retries, compaction, steering, follow-ups), so only `agent_settled` returns the session to
 *   `idle`.
 *
 * Everything else pi sends — unknown record types, extension UI traffic, `tool_execution_update`,
 * `compaction_*` — is logged and dropped. Protocol drift is a log line, never a dead session.
 *
 * Deliberate limits, mirrored by the Rust adapter's `Capabilities`:
 *
 * - **No interactive permissions.** pi's tools run without asking; the only request/answer surface
 *   is the *extension UI* sub-protocol, which is logged rather than half-supported. Hence
 *   `capabilities.permissions = false` and `session.permission` answers `unsupported`.
 * - **No diffs.** pi sends no patch for edits; synthesizing one from tool arguments is future work.
 * - **Resume is the session file path.** It comes from `get_session_stats.sessionFile` and is
 *   reopened with `switch_session`.
 *
 * An optional in-process SDK path is documented in `docs/sidecar-protocol.md` as future work; the
 * protocol above it does not change if it is ever built.
 */

import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { statSync } from "node:fs";
import { delimiter, isAbsolute, join } from "node:path";
import {
  delta as deltaEvent,
  errorEvent,
  message as messageEvent,
  sessionStarted,
  status as statusEvent,
  toolCall as toolCallEvent,
  toolResult as toolResultEvent,
  usage as usageEvent,
  type Block,
  type HarnessEvent,
  type SessionState,
} from "./events.js";
import type { SidecarNotifications } from "./sidecar.js";
import {
  PROTOCOL_VERSION,
  RPC_INTERNAL,
  RPC_NOT_FOUND,
  RPC_UNAVAILABLE,
  RPC_UNSUPPORTED,
  SIDECAR_VERSION,
  SidecarFailure,
  type InitializeResult,
  type PiLaunchConfig,
  type SessionPermissionParams,
  type SessionRef,
  type SessionSendParams,
  type SessionStartParams,
  type SessionStartResult,
  type SidecarHost,
} from "./rpc.js";
import { createLineSplitter } from "./ndjson.js";

const HARNESS = "pi";
/** pi's documented programmatic surface: JSONL over stdio. */
const DEFAULT_ARGS = ["--mode", "rpc"];
const DEFAULT_COMMAND = "pi";
/** A tool result can be a whole file; the transcript wants a summary. */
const SUMMARY_LIMIT = 500;
/** Startup requests are bounded so a broken pi cannot hold `session.start` forever. */
const CONTROL_TIMEOUT_MS = 5000;
/** The prompt response only says "accepted"; the turn itself is announced by events. */
const PROMPT_TIMEOUT_MS = 15_000;
/** How long a cancel waits for the `agent_settled` its `abort` produces. */
const SETTLE_TIMEOUT_MS = 3000;

export interface PiHostOptions {
  notify: SidecarNotifications;
  /** Default launch configuration. `session.start`'s `config.pi` overrides it per session. */
  config?: PiLaunchConfig;
  /** Diagnostics sink; defaults to stderr, which is where everything but protocol must go. */
  log?: (message: string) => void;
}

interface ResolvedConfig {
  command: string;
  args: string[];
  sessionDir?: string;
}

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timer: NodeJS.Timeout;
}

type PiRecord = Record<string, unknown>;

interface LiveSession {
  id: string;
  child: ChildProcessWithoutNullStreams;
  state: SessionState;
  resumeToken?: string;
  /** Set while the session is being torn down, so the exit is not reported as a crash. */
  stopping: boolean;
  running: boolean;
  /** The current turn was cancelled: its remaining frames must not reach the transcript. */
  aborted: boolean;
  queue: string[];
  pending: Map<string, Pending>;
  nextId: number;
  deltaText: string;
  thinkingText: string;
  usage: { input: number; output: number };
  /** `agent_settled` frames seen, so a cancel can wait for the one its abort produces. */
  settles: number;
}

export function createPiHost(options: PiHostOptions): SidecarHost {
  return new PiHost(options);
}

export class PiHost implements SidecarHost {
  private readonly sessions = new Map<string, LiveSession>();
  private readonly notify: SidecarNotifications;
  private readonly defaults: ResolvedConfig;
  private readonly log: (message: string) => void;

  constructor(options: PiHostOptions) {
    this.notify = options.notify;
    this.defaults = resolveConfig(options.config);
    this.log =
      options.log ??
      ((message) => process.stderr.write(`grove-sidecar: ${message}\n`));
  }

  info(): InitializeResult {
    // Availability describes the *default* resolution. `session.start` re-checks against its own
    // `config.pi`, which is authoritative for that session.
    const found = resolveCommand(this.defaults.command);
    return {
      protocolVersion: PROTOCOL_VERSION,
      version: SIDECAR_VERSION,
      harnesses: [HARNESS],
      pi: found
        ? { available: true, command: found }
        : {
            available: false,
            command: this.defaults.command,
            reason: missingBinaryReason(this.defaults.command),
          },
    };
  }

  async start(params: SessionStartParams): Promise<SessionStartResult> {
    const config = resolveConfig(params.config?.pi, this.defaults);
    const binary = resolveCommand(config.command);
    if (!binary) {
      throw new SidecarFailure(RPC_UNAVAILABLE, missingBinaryReason(config.command), {
        reason: missingBinaryReason(config.command),
      });
    }

    const args = launchArgs(config, params.model);
    let child: ChildProcessWithoutNullStreams;
    try {
      child = spawn(binary, args, {
        cwd: params.cwd,
        stdio: ["pipe", "pipe", "pipe"],
        windowsHide: true,
      });
      await spawned(child);
    } catch (error) {
      throw new SidecarFailure(
        RPC_UNAVAILABLE,
        `could not launch pi (\`${binary} ${args.join(" ")}\`): ${errorMessage(error)}`,
        { reason: errorMessage(error) },
      );
    }
    // A write to a dead child must not crash the sidecar: the exit handler reports it instead.
    child.stdin.on("error", (error) => this.log(`writing to pi failed: ${error.message}`));
    child.stdout.on("error", (error) => this.log(`reading pi's stdout failed: ${error.message}`));

    const session: LiveSession = {
      id: params.sessionId,
      child,
      state: "starting",
      resumeToken: undefined,
      stopping: false,
      running: false,
      aborted: false,
      queue: [],
      pending: new Map(),
      nextId: 0,
      deltaText: "",
      thinkingText: "",
      usage: { input: 0, output: 0 },
      settles: 0,
    };
    this.sessions.set(session.id, session);

    const splitter = createLineSplitter((line) => this.handleLine(session, line));
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => splitter.push(chunk));
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      for (const line of String(chunk).split("\n")) {
        if (line.trim().length > 0) this.log(`pi: ${line.trim()}`);
      }
    });
    // `close` (not `exit`) so everything pi wrote is mapped before the session is declared dead.
    child.once("close", (code, signal) => {
      splitter.end();
      this.onExit(session, code, signal);
    });

    try {
      if (params.resumeToken) {
        // Reopening a conversation by its session file path. A missing file is pi's call to make:
        // the failure is logged and the session starts as a fresh conversation rather than dying.
        try {
          await this.request(session, {
            type: "switch_session",
            sessionPath: params.resumeToken,
          });
          session.resumeToken = params.resumeToken;
        } catch (error) {
          this.log(`pi refused to reopen ${params.resumeToken}: ${errorMessage(error)}`);
        }
      }
      const stats = await this.sessionFile(session);
      if (stats) session.resumeToken = stats;
    } catch (error) {
      // A control request that pi never answers is not a reason to fail the session: the turn path
      // still works, only the resume token is missing.
      this.log(`pi did not report its session file: ${errorMessage(error)}`);
    }

    this.emit(
      session.id,
      sessionStarted(HARNESS, params.model ?? null, Boolean(params.resumeToken)),
    );
    this.emitStatus(session, "starting");
    this.emitStatus(session, "ready");

    const first = composeFirstPrompt(params);
    if (first) {
      session.queue.push(first);
      // Let the `session.start` response reach the parent before the first turn's events arrive.
      setImmediate(() => this.pump(session));
    }

    return session.resumeToken ? { resumeToken: session.resumeToken } : {};
  }

  async send(params: SessionSendParams): Promise<void> {
    const session = this.require(params.sessionId);
    session.queue.push(params.text);
    // Deferred so the `session.send` response reaches the parent before the turn's first event.
    setImmediate(() => this.pump(session));
  }

  async permission(params: SessionPermissionParams): Promise<void> {
    throw new SidecarFailure(
      RPC_UNSUPPORTED,
      `pi has no tool-approval gate: its tools run without asking (session ${params.sessionId}, ` +
        `request ${params.requestId}). Configure the harness itself instead.`,
    );
  }

  async cancel(params: SessionRef): Promise<void> {
    const session = this.sessions.get(params.sessionId);
    // Cancelling a session that is not running is a no-op that still confirms the idle state.
    if (!session) return;
    if (!session.running) {
      if (session.state !== "idle" && session.state !== "exited") {
        this.emitStatus(session, "idle");
      }
      return;
    }
    session.aborted = true;
    // There is nothing to steer any more: a cancel drops prompts that were waiting behind the turn.
    session.queue.length = 0;
    const settled = session.settles;
    try {
      // `abort` is a request: the acknowledgement is what makes this a cancel and not a kill.
      await this.request(session, { type: "abort" });
    } catch (error) {
      this.log(`pi did not acknowledge the abort: ${errorMessage(error)}`);
    }
    // `agent_settled` is pi's idle signal, so a cancel waits for it instead of racing it: a settle
    // that arrived after the next prompt would end that turn before it started. If pi never settles
    // (an older build, a wedged run) the session still returns to idle once the grace period ends.
    await this.waitForSettle(session, settled);
    session.running = false;
    if (session.state !== "idle" && session.state !== "exited") {
      this.emitStatus(session, "idle");
    }
  }

  private async waitForSettle(session: LiveSession, before: number): Promise<void> {
    const deadline = Date.now() + SETTLE_TIMEOUT_MS;
    while (session.settles <= before && session.state !== "exited" && Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }

  async stop(params: SessionRef): Promise<void> {
    const session = this.sessions.get(params.sessionId);
    if (!session) return;
    this.sessions.delete(params.sessionId);
    this.teardown(session, "the session was stopped");
  }

  async shutdown(): Promise<void> {
    for (const session of [...this.sessions.values()]) {
      this.sessions.delete(session.id);
      this.teardown(session, "the sidecar is shutting down");
    }
  }

  // --- mapping -------------------------------------------------------------

  private handleLine(session: LiveSession, line: string): void {
    let decoded: unknown;
    try {
      decoded = JSON.parse(line);
    } catch {
      this.log(`dropping a malformed pi record: ${truncate(line, 200)}`);
      return;
    }
    const record = asRecord(decoded);
    if (!record) {
      this.log(`dropping a non-object pi record: ${truncate(line, 200)}`);
      return;
    }

    switch (asString(record.type)) {
      case "response":
        this.settle(session, record);
        return;
      case "agent_start":
        // pi started an agent run. `busy` was already announced when the prompt was sent; a run we
        // did not ask for (a follow-up, a retry) still wants the status.
        if (session.state !== "busy") this.emitStatus(session, "busy");
        return;
      case "message_update":
        this.handleMessageUpdate(session, record);
        return;
      case "message_end": {
        const message = asRecord(record.message);
        if (message && asString(message.role) === "assistant") {
          this.emitAssistantMessage(session, message);
        }
        return;
      }
      case "tool_execution_start":
        this.emitToolCall(session, record);
        return;
      case "tool_execution_end":
        this.emitToolResult(session, record);
        return;
      case "tool_execution_update":
        // Progress frames are optional in the mapping and fold into the final result.
        this.log("ignoring a pi tool progress update");
        return;
      case "agent_settled": {
        // The one idle signal. `aborted: true` still means idle. `session.aborted` stays set until
        // the next prompt so the cancelled turn's remaining frames are dropped.
        session.settles += 1;
        session.running = false;
        session.deltaText = "";
        session.thinkingText = "";
        if (session.state !== "idle") this.emitStatus(session, "idle");
        this.pump(session);
        return;
      }
      case "turn_end": {
        const message = asRecord(record.message);
        const usage = message ? asRecord(message.usage) : undefined;
        if (usage) this.noteUsage(session, usage);
        return;
      }
      case "auto_retry_start": {
        const detail = asString(record.errorMessage) ?? "an error";
        const attempt = asNumber(record.attempt);
        const max = asNumber(record.maxAttempts);
        const suffix =
          attempt !== undefined && max !== undefined ? ` (attempt ${attempt}/${max})` : "";
        this.emit(session.id, errorEvent(`pi is retrying after ${detail}${suffix}`, true));
        if (session.state !== "busy") this.emitStatus(session, "busy");
        return;
      }
      case "auto_retry_end":
      case "compaction_start":
      case "compaction_end":
      case "turn_start":
      case "message_start":
      case "agent_end":
        // Lifecycle bookkeeping only: none of these is content, and `agent_end` is not the end.
        this.log(`ignoring pi record: ${asString(record.type)}`);
        return;
      case "extension_ui_request":
      case "extension_error":
        // pi's only request/answer surface, used by extensions. Grove does not answer it (see the
        // module docs): logged, not half-supported.
        this.log(`ignoring pi extension traffic: ${asString(record.type)}`);
        return;
      default:
        this.log(`dropping an unknown pi record type: ${asString(record.type) ?? "<missing>"}`);
    }
  }

  private handleMessageUpdate(session: LiveSession, record: PiRecord): void {
    const usage = asRecord(record.usage);
    if (usage) this.noteUsage(session, usage);

    const event = asRecord(record.assistantMessageEvent);
    const type = event ? asString(event.type) : undefined;
    switch (type) {
      case "text_delta": {
        const text = event ? asString(event.delta) : undefined;
        if (!text) return;
        if (session.aborted) return;
        session.deltaText += text;
        this.emit(session.id, deltaEvent(text));
        return;
      }
      case "thinking_delta": {
        const text = event ? asString(event.delta) : undefined;
        if (text) session.thinkingText += text;
        return;
      }
      case "thinking_end":
        // The thinking text rides along with the assistant message `message_end` produces, so a turn
        // is one message with [thinking, text] blocks — the same shape the Claude adapter emits. It
        // is never a message of its own, which would arrive before the streamed deltas.
        return;
      case "start":
      case "text_start":
      case "text_end":
      case "thinking_start":
      case "toolcall_start":
      case "toolcall_delta":
      case "toolcall_end":
      case "done":
        return;
      case "error": {
        const reason = asString(event?.error) ?? "pi reported a stream error";
        this.emit(session.id, errorEvent(reason, true));
        return;
      }
      default:
        this.log(`ignoring an unknown pi message event: ${type ?? "<missing>"}`);
    }
  }

  private emitAssistantMessage(session: LiveSession, message: PiRecord): void {
    const blocks: Block[] = [];
    for (const item of asArray(message.content) ?? []) {
      const block = asRecord(item);
      const type = block ? asString(block.type) : undefined;
      if (type === "text") {
        const text = asString(block?.text);
        if (text) blocks.push({ type: "text", text });
      } else if (type === "thinking") {
        const text = asString(block?.thinking) ?? asString(block?.text);
        if (text) blocks.push({ type: "thinking", text });
      }
    }
    // `message_end` is authoritative; the buffered deltas are discarded. If pi stripped the content
    // from the wire, the deltas are the only copy of the answer left.
    const thinking = session.thinkingText.trim();
    if (thinking && !blocks.some((block) => block.type === "thinking")) {
      blocks.unshift({ type: "thinking", text: thinking });
    }
    if (blocks.length === 0 && session.deltaText) {
      blocks.push({ type: "text", text: session.deltaText });
    }
    session.deltaText = "";
    session.thinkingText = "";
    if (blocks.length === 0 || session.aborted) return;
    this.emit(session.id, messageEvent("assistant", blocks));
  }

  private emitToolCall(session: LiveSession, record: PiRecord): void {
    if (session.aborted) return;
    const callId = asString(record.toolCallId);
    if (!callId) {
      this.log("dropping a pi tool_execution_start without a toolCallId");
      return;
    }
    const name = asString(record.toolName) ?? "tool";
    const args = asRecord(record.args);
    const title = firstString(args, ["command", "path", "file_path", "description"]) ?? name;
    this.emit(session.id, toolCallEvent(callId, name, truncate(title, 200)));
  }

  private emitToolResult(session: LiveSession, record: PiRecord): void {
    if (session.aborted) return;
    const callId = asString(record.toolCallId);
    if (!callId) {
      this.log("dropping a pi tool_execution_end without a toolCallId");
      return;
    }
    const result = asRecord(record.result);
    const summary = summarize(result ? result.content : undefined);
    this.emit(
      session.id,
      toolResultEvent(callId, record.isError === true ? "error" : "ok", summary),
    );
  }

  private noteUsage(session: LiveSession, usage: PiRecord): void {
    const input = asNumber(usage.input) ?? 0;
    const output = asNumber(usage.output) ?? 0;
    // pi fills `usage` with zeros until the provider reports it, and repeats the last value on
    // several frames: only a real change is worth a transcript line.
    if (input === 0 && output === 0) return;
    if (input === session.usage.input && output === session.usage.output) return;
    session.usage = { input, output };
    this.emit(session.id, usageEvent(input, output));
  }

  // --- turn lifecycle ------------------------------------------------------

  private pump(session: LiveSession): void {
    if (session.running || session.queue.length === 0 || session.stopping) return;
    if (session.state === "exited") return;
    const text = session.queue.shift();
    if (!text) return;

    session.running = true;
    session.aborted = false;
    session.deltaText = "";
    session.thinkingText = "";
    session.usage = { input: 0, output: 0 };
    this.emitStatus(session, "busy");

    this.request(session, { type: "prompt", message: text }, PROMPT_TIMEOUT_MS).catch((error) => {
      this.log(`pi did not accept the prompt: ${errorMessage(error)}`);
      session.running = false;
      this.emit(session.id, errorEvent(`pi did not accept the prompt: ${errorMessage(error)}`, true));
      if (session.state !== "exited") this.emitStatus(session, "idle");
    });
  }

  /** Property order and a stable view: status transitions are recorded before they are emitted. */
  private emitStatus(session: LiveSession, state: SessionState): void {
    session.state = state;
    this.emit(session.id, statusEvent(state));
  }

  private emit(sessionId: string, event: HarnessEvent): void {
    this.notify.event(sessionId, event);
  }

  // --- pi process ----------------------------------------------------------

  /** Send one command and wait for pi's matching response (correlated by `id`). */
  private request(session: LiveSession, command: PiRecord, timeoutMs = CONTROL_TIMEOUT_MS): Promise<unknown> {
    return new Promise<unknown>((resolve, reject) => {
      const id = `${session.id}:${session.nextId++}`;
      const timer = setTimeout(() => {
        session.pending.delete(id);
        reject(
          new SidecarFailure(
            RPC_UNAVAILABLE,
            `pi did not answer ${asString(command.type) ?? "a command"} within ${timeoutMs}ms`,
          ),
        );
      }, timeoutMs);
      session.pending.set(id, { resolve, reject, timer });
      const line = `${JSON.stringify({ id, ...command })}\n`;
      // Commands are tiny, so a full pipe is a symptom rather than a queueing problem: the exit
      // handler reports it, and the request above times out with a reason either way.
      if (!session.child.stdin.write(line)) {
        this.log("pi's stdin is full; backpressure is not queued (commands are small)");
      }
    });
  }

  private settle(session: LiveSession, record: PiRecord): void {
    const id = record.id;
    if (typeof id !== "string" && typeof id !== "number") {
      this.log("dropping a pi response without an id");
      return;
    }
    const pending = session.pending.get(String(id));
    if (!pending) {
      this.log(`dropping a pi response for an unknown request: ${String(id)}`);
      return;
    }
    session.pending.delete(String(id));
    clearTimeout(pending.timer);
    if (record.success === false) {
      pending.reject(
        new SidecarFailure(
          RPC_INTERNAL,
          asString(record.error) ?? `pi rejected ${asString(record.command) ?? "a command"}`,
        ),
      );
      return;
    }
    pending.resolve(record.data);
  }

  /** `data.sessionFile` — pi's resumable handle, and therefore the resume token. */
  private async sessionFile(session: LiveSession): Promise<string | undefined> {
    const data = asRecord(await this.request(session, { type: "get_session_stats" }));
    const file = data ? asString(data.sessionFile) : undefined;
    return file && file.length > 0 ? file : undefined;
  }

  private onExit(
    session: LiveSession,
    code: number | null,
    signal: NodeJS.Signals | null,
  ): void {
    if (session.stopping || !this.sessions.has(session.id)) return;
    this.sessions.delete(session.id);
    session.running = false;
    this.failPending(session, "the pi process exited");
    const how = code !== null ? `exit code ${code}` : `signal ${signal ?? "unknown"}`;
    this.emit(session.id, errorEvent(`the pi process exited before the turn settled (${how})`, true));
    this.emitStatus(session, "exited");
  }

  private teardown(session: LiveSession, reason: string): void {
    session.stopping = true;
    session.queue.length = 0;
    this.failPending(session, reason);
    try {
      session.child.kill();
    } catch (error) {
      this.log(`killing pi failed: ${errorMessage(error)}`);
    }
  }

  private failPending(session: LiveSession, reason: string): void {
    for (const [id, pending] of session.pending) {
      clearTimeout(pending.timer);
      session.pending.delete(id);
      pending.reject(new SidecarFailure(RPC_UNAVAILABLE, reason));
    }
  }

  private require(sessionId: string): LiveSession {
    const session = this.sessions.get(sessionId);
    if (!session) {
      throw new SidecarFailure(RPC_NOT_FOUND, `no such session: ${sessionId}`);
    }
    return session;
  }
}

/** `session.start.config.pi`, falling back to the sidecar's own defaults for absent fields. */
function resolveConfig(pi?: PiLaunchConfig, defaults?: ResolvedConfig): ResolvedConfig {
  const fallback = defaults ?? { command: DEFAULT_COMMAND, args: [...DEFAULT_ARGS] };
  const command =
    typeof pi?.command === "string" && pi.command.trim().length > 0 ? pi.command : fallback.command;
  const configured =
    Array.isArray(pi?.args) && pi.args.length > 0
      ? pi.args.filter((arg): arg is string => typeof arg === "string")
      : undefined;
  const args = configured ?? [...fallback.args];
  const sessionDir =
    typeof pi?.sessionDir === "string" && pi.sessionDir.trim().length > 0
      ? pi.sessionDir
      : undefined;
  return { command, args, sessionDir };
}

/** The argv pi is launched with: the user's args, plus `--session-dir`/`--model` when unset. */
function launchArgs(config: ResolvedConfig, model?: string): string[] {
  const args = [...config.args];
  if (config.sessionDir && !args.includes("--session-dir")) {
    args.push("--session-dir", config.sessionDir);
  }
  if (model && !args.includes("--model")) {
    args.push("--model", model);
  }
  return args;
}

/** Resolve a binary the way a shell would: an explicit path as given, otherwise `PATH` + `PATHEXT`. */
export function resolveCommand(command: string): string | undefined {
  if (command.includes("/") || command.includes("\\") || isAbsolute(command)) {
    return isFile(command) ? command : undefined;
  }
  const pathEnv = process.env.PATH ?? "";
  const extensions =
    process.platform === "win32"
      ? (process.env.PATHEXT ?? ".COM;.EXE;.BAT;.CMD")
          .split(";")
          .map((extension) => extension.trim().toLowerCase())
          .filter((extension) => extension.length > 0)
      : [""];
  for (const directory of pathEnv.split(delimiter)) {
    if (!directory) continue;
    for (const extension of extensions) {
      const candidate = join(directory, `${command}${extension}`);
      if (isFile(candidate)) return candidate;
    }
  }
  return undefined;
}

function isFile(path: string): boolean {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

function missingBinaryReason(command: string): string {
  return `pi is not installed or not on PATH (looked for \`${command}\`); install pi or set config.pi.command`;
}

/** Resolve once the child is really running; `spawn` reports ENOENT asynchronously. */
function spawned(child: ChildProcessWithoutNullStreams): Promise<void> {
  return new Promise((resolve, reject) => {
    const onSpawn = () => {
      child.off("error", onError);
      resolve();
    };
    const onError = (error: Error) => {
      child.off("spawn", onSpawn);
      reject(error);
    };
    child.once("spawn", onSpawn);
    child.once("error", onError);
  });
}

/**
 * The first turn's text: the pinned specs and plan (already rendered to markdown by Rust) followed by
 * the prompt, in the same shape the other adapters prepend context in.
 */
function composeFirstPrompt(params: SessionStartParams): string | undefined {
  const prompt = params.prompt?.trim();
  const docs = params.context ?? [];
  if (docs.length === 0) return prompt && prompt.length > 0 ? prompt : undefined;
  const context = docs
    .map((doc) => `## ${doc.title}\n\n<!-- id: ${doc.id} -->\n\n${doc.body.trimEnd()}\n`)
    .join("\n");
  const rendered = `# Context\n\n${context}`;
  return prompt && prompt.length > 0 ? `${rendered}\n---\n\n${prompt}` : rendered;
}

function asRecord(value: unknown): PiRecord | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as PiRecord)
    : undefined;
}

function asArray(value: unknown): unknown[] | undefined {
  return Array.isArray(value) ? value : undefined;
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function firstString(record: PiRecord | undefined, keys: string[]): string | undefined {
  if (!record) return undefined;
  for (const key of keys) {
    const value = asString(record[key]);
    if (value && value.trim().length > 0) return value.trim();
  }
  return undefined;
}

function summarize(content: unknown): string {
  if (typeof content === "string") return truncate(content, SUMMARY_LIMIT);
  const items = asArray(content);
  if (!items) {
    return content === undefined || content === null ? "" : truncate(JSON.stringify(content), SUMMARY_LIMIT);
  }
  const text = items
    .map((item) => asRecord(item))
    .filter((item): item is PiRecord => item !== undefined)
    .map((item) => asString(item.text) ?? "")
    .filter((part) => part.length > 0)
    .join("\n");
  return truncate(text, SUMMARY_LIMIT);
}

function truncate(text: string, limit: number): string {
  if (text.length <= limit) return text;
  return `${text.slice(0, limit)}…`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
