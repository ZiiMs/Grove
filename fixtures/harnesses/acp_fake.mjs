#!/usr/bin/env node
// A scripted stand-in for **any** Agent Client Protocol agent: JSON-RPC 2.0 over stdio, one JSON
// object per line, no network and no model. It lets the ACP adapter tests run offline and without a
// real agent installed, and it is the oracle for the protocol details Grove promises to speak:
//
//   initialize            pins `protocolVersion: 1`, validates `clientInfo`
//   session/new           validates `cwd` (the directory the agent really runs in) and `mcpServers`
//   session/resume        continues a session without replaying it; `session/load` is rejected
//   session/prompt        one long-lived request per turn, answered with a StopReason
//   session/cancel        a notification; the pending permission must be answered `cancelled`
//   session/update        every variant the adapter maps, plus variants it must ignore
//   session/request_permission  a request the client must always answer
//
// Modes:
//   happy             text chunks, a thought chunk, two tool calls (one ok, one failed), a plan, and
//                     unknown/ignored update variants, then `stopReason: "end_turn"`
//   permission        a chunk, then `session/request_permission`; the answer decides the rest
//   cancel            a pending permission and a slow chunk stream; the turn is cut short by
//                     `session/cancel` and only ends once the permission was answered `cancelled`
//   version-mismatch  `initialize` answers `protocolVersion: 2` (Grove pins version 1)
//   unknown-line      junk lines, an unknown notification and an unknown *request* interleaved with
//                     a valid turn; the request must be answered or the fake gives up
//   crash             a chunk, then the process dies mid-turn without answering the prompt
//
// A wrong protocol fails loudly, so the adapter's wire shape is tested instead of assumed:
//   exit 2  bad argv, a client that does not pin protocolVersion 1, or a malformed session request
//   exit 3  the client continued after a protocol-version mismatch
//   exit 4  the client used session/load (it replays the whole transcript; use session/resume)
//   exit 5  a permission answer that is not the documented RequestPermissionOutcome shape
//   exit 6  a request or permission the client never answered

import fs from "node:fs";
import path from "node:path";

const MODES = new Set([
  "happy",
  "permission",
  "cancel",
  "version-mismatch",
  "unknown-line",
  "crash",
]);
/** How long a request the client must answer may stay unanswered before the fixture gives up. */
const ANSWER_TIMEOUT_MS = 10_000;

const argv = process.argv.slice(2);

function flag(name, fallback = null) {
  const index = argv.indexOf(name);
  return index >= 0 && index + 1 < argv.length ? argv[index + 1] : fallback;
}

function die(code, message) {
  process.stderr.write(`acp_fake: ${message}\n`);
  process.exit(code);
}

const mode = flag("--mode");
if (!mode) die(2, `missing --mode (one of: ${[...MODES].join(", ")})`);
if (!MODES.has(mode)) die(2, `unknown --mode: ${mode}`);
const rawDelay = flag("--delay-ms", "0");
const delayMs = Number(rawDelay);
if (!Number.isFinite(delayMs) || delayMs < 0) die(2, `--delay-ms must be a non-negative number, got ${rawDelay}`);

/** The session this process serves. `session/new` mints it, `session/resume` restores it. */
let sessionId = null;
let initialized = false;
let promptCount = 0;
/** The turn in flight: `{ id, cancelled, onCancel }`. Grove sends one prompt at a time. */
let turn = null;
let nextPermissionId = 5;
/** jsonrpc id -> `{ resolve, options }` for `session/request_permission` requests. */
const pendingPermissions = new Map();
/** jsonrpc id -> resolve, for requests the client is expected to answer with an error. */
const pendingAnswers = new Map();

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function respond(id, result) {
  write({ jsonrpc: "2.0", id, result });
}

function respondError(id, code, message) {
  write({ jsonrpc: "2.0", id, error: { code, message } });
}

function notify(method, params) {
  write({ jsonrpc: "2.0", method, params });
}

/** One `session/update` notification — the only way an agent reports work. */
function update(body) {
  notify("session/update", { sessionId, update: body });
}

function pause(ms) {
  return ms > 0 ? new Promise((resolve) => setTimeout(resolve, ms)) : Promise.resolve();
}

function firstLine(text) {
  return text.split(/\r?\n/).find((line) => line.trim().length > 0)?.trim() ?? "";
}

function promptText(params) {
  const blocks = params?.prompt;
  if (!Array.isArray(blocks) || blocks.length === 0) {
    die(2, "session/prompt must carry a non-empty `prompt` array");
  }
  const parts = [];
  for (const block of blocks) {
    if (block?.type !== "text" || typeof block.text !== "string") {
      die(2, `only text prompt blocks are supported here, got ${JSON.stringify(block)}`);
    }
    parts.push(block.text);
  }
  return parts.join("\n");
}

/**
 * The spec: `cwd` MUST be an absolute path and MUST be used for the session regardless of where the
 * agent was spawned. Grove sets both, so comparing them proves the adapter passes the real directory.
 */
function validateSessionParams(params, method) {
  if (typeof params?.cwd !== "string" || params.cwd.trim().length === 0) {
    die(2, `${method} must carry an absolute \`cwd\``);
  }
  if (!Array.isArray(params.mcpServers)) {
    die(2, `${method} must carry an \`mcpServers\` array`);
  }
  const normalize = (value) =>
    (process.platform === "win32" ? value.toLowerCase() : value).replace(/\\/g, "/");
  let wanted;
  let actual;
  try {
    wanted = fs.realpathSync(params.cwd);
  } catch {
    die(2, `${method} cwd does not exist: ${params.cwd}`);
  }
  try {
    actual = fs.realpathSync(process.cwd());
  } catch {
    die(2, `the agent's own working directory is invalid: ${process.cwd()}`);
  }
  if (normalize(path.resolve(wanted)) !== normalize(path.resolve(actual))) {
    die(2, `${method} cwd ${params.cwd} is not the directory the agent runs in (${process.cwd()})`);
  }
}

function handleInitialize(message) {
  const params = message.params ?? {};
  if (params.protocolVersion !== 1) {
    die(2, `the client must pin protocolVersion 1, got ${JSON.stringify(params.protocolVersion)}`);
  }
  if (typeof params.clientInfo?.name !== "string" || params.clientInfo.name.length === 0) {
    die(2, "initialize must include a clientInfo.name");
  }
  initialized = true;
  if (mode === "version-mismatch") {
    // Version negotiation: the agent answers with the newest version it speaks. Grove must notice
    // that it cannot speak version 2 and end the session instead of carrying on.
    respond(message.id, {
      protocolVersion: 2,
      agentCapabilities: {},
      authMethods: [],
      agentInfo: { name: "acp_fake", title: "ACP fake (v2)", version: "2.0.0" },
    });
    return;
  }
  respond(message.id, {
    protocolVersion: 1,
    agentCapabilities: {
      // Loading replays the whole conversation; resuming does not. The adapter must prefer resume.
      loadSession: true,
      promptCapabilities: { image: false, audio: false, embeddedContext: false },
      mcpCapabilities: { http: false, sse: false },
      sessionCapabilities: { resume: {} },
    },
    authMethods: [],
    agentInfo: { name: "acp_fake", title: "ACP fake", version: "0.1.0" },
  });
}

function handleNew(message) {
  if (!initialized) die(2, "session/new before initialize");
  if (mode === "version-mismatch") die(3, "the client continued after a protocol-version mismatch");
  validateSessionParams(message.params, "session/new");
  sessionId = `fake-acp-session-${process.pid}-${Date.now()}`;
  respond(message.id, { sessionId, modes: { currentModeId: "default", availableModes: [] } });
}

function handleResume(message) {
  if (!initialized) die(2, "session/resume before initialize");
  if (mode === "version-mismatch") die(3, "the client continued after a protocol-version mismatch");
  validateSessionParams(message.params, "session/resume");
  const requested = message.params?.sessionId;
  if (typeof requested !== "string" || requested.length === 0) {
    die(2, "session/resume must carry the session id to continue");
  }
  // No session/update replay: that is the whole point of resume over load.
  sessionId = requested;
  respond(message.id, { sessionId });
}

/** A permission request the client must answer, with the answer validated against the spec shape. */
function askPermission(current, toolCall, options) {
  const id = nextPermissionId;
  nextPermissionId += 1;
  write({
    jsonrpc: "2.0",
    id,
    method: "session/request_permission",
    params: { sessionId, toolCall, options },
  });
  return new Promise((resolve) => {
    const timer = setTimeout(
      () => die(6, `session/request_permission ${id} was never answered (the client must always answer)`),
      ANSWER_TIMEOUT_MS,
    );
    pendingPermissions.set(id, {
      options,
      resolve: (answer) => {
        clearTimeout(timer);
        pendingPermissions.delete(id);
        resolve(answer);
      },
    });
    // The turn may be cancelled before the answer lands; the answer still has to arrive.
    current.permissionId = id;
  });
}

function validatePermissionAnswer(message, options) {
  if (message.error) {
    die(5, `session/request_permission must be answered with a result, got ${JSON.stringify(message.error)}`);
  }
  const outcome = message.result?.outcome;
  if (!outcome || typeof outcome !== "object") {
    die(5, `request_permission result must be {outcome:{...}}, got ${JSON.stringify(message.result)}`);
  }
  if (outcome.outcome === "selected") {
    if (typeof outcome.optionId !== "string" || outcome.optionId.length === 0) {
      die(5, "a `selected` outcome must carry an `optionId`");
    }
    if (!options.some((option) => option.optionId === outcome.optionId)) {
      die(5, `selected an option that was never offered: ${outcome.optionId}`);
    }
    return { outcome: "selected", optionId: outcome.optionId };
  }
  if (outcome.outcome === "cancelled") {
    return { outcome: "cancelled" };
  }
  die(5, `unknown permission outcome: ${JSON.stringify(outcome.outcome)}`);
}

/** Wait for a cancel notification, or for `ms` to pass. */
function waitForCancel(ms, current) {
  if (current.cancelled) return Promise.resolve("cancelled");
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      current.onCancel = null;
      resolve("timeout");
    }, ms);
    current.onCancel = () => {
      clearTimeout(timer);
      current.onCancel = null;
      resolve("cancelled");
    };
  });
}

/** Wait until the client answers the request `id` (any answer: the point is that it is not ignored). */
function awaitAnswer(id, what) {
  return new Promise((resolve) => {
    pendingAnswers.set(id, resolve);
    setTimeout(() => die(6, `${what} was never answered`), ANSWER_TIMEOUT_MS);
  });
}

function handleResponse(message) {
  const permission = pendingPermissions.get(message.id);
  if (permission) {
    permission.resolve(validatePermissionAnswer(message, permission.options));
    return;
  }
  const answer = pendingAnswers.get(message.id);
  if (answer) {
    pendingAnswers.delete(message.id);
    answer(message);
    return;
  }
  process.stderr.write(`acp_fake: ignoring a response for an unknown id: ${message.id}\n`);
}

async function runTurn(current, text) {
  const echo = firstLine(text);
  switch (mode) {
    case "happy": {
      update({ sessionUpdate: "agent_message_chunk", messageId: "msg_1", content: { type: "text", text: "Working on it: " } });
      update({ sessionUpdate: "agent_thought_chunk", messageId: "msg_1", content: { type: "text", text: "A short answer is enough." } });
      update({ sessionUpdate: "agent_message_chunk", messageId: "msg_1", content: { type: "text", text: `Fake reply: ${echo}` } });
      // A replay echo: only `session/load` produces these, so they must be ignored.
      update({ sessionUpdate: "user_message_chunk", messageId: "echo", content: { type: "text", text: echo } });
      update({ sessionUpdate: "plan", entries: [{ content: "a plan the adapter ignores for now", priority: "low", status: "pending" }] });
      update({ sessionUpdate: "tool_call", toolCallId: "call_001", name: "read_file", title: "Reading the configuration file", kind: "read", status: "pending" });
      update({ sessionUpdate: "tool_call_update", toolCallId: "call_001", status: "in_progress" });
      update({ sessionUpdate: "tool_call_update", toolCallId: "call_001", status: "completed", content: [{ type: "content", content: { type: "text", text: "config loaded" } }] });
      update({ sessionUpdate: "tool_call", toolCallId: "call_002", name: "bash", title: "Run: cargo test -p grove-core", kind: "execute", status: "pending" });
      update({ sessionUpdate: "tool_call_update", toolCallId: "call_002", status: "failed", content: [{ type: "content", content: { type: "text", text: "error: one test failed" } }] });
      // Variants the adapter must ignore without dying.
      update({ sessionUpdate: "usage_update", used: 1200, size: 200000 });
      update({ sessionUpdate: "current_mode_update", currentModeId: "default" });
      update({ sessionUpdate: "available_commands_update", commands: [] });
      update({ sessionUpdate: "session_info_update", title: "acp fake" });
      update({ sessionUpdate: "from_the_future", payload: { anything: true } });
      update({ sessionUpdate: "agent_message_chunk", messageId: "msg_2", content: { type: "text", text: "All done." } });
      await pause(delayMs);
      respond(current.id, { stopReason: "end_turn" });
      return;
    }

    case "permission": {
      update({ sessionUpdate: "agent_message_chunk", messageId: "msg_1", content: { type: "text", text: "I need to run a command. " } });
      const options = [
        { optionId: "allow-once", name: "Allow once", kind: "allow_once" },
        { optionId: "allow-always", name: "Allow always", kind: "allow_always" },
        { optionId: "reject-once", name: "Reject", kind: "reject_once" },
      ];
      const answer = await askPermission(current, { toolCallId: "call_perm", title: "Run: rm -rf build", kind: "execute", status: "pending" }, options);
      if (answer.outcome === "selected") {
        update({ sessionUpdate: "agent_message_chunk", messageId: "msg_1", content: { type: "text", text: `Selected ${answer.optionId}.` } });
        update({ sessionUpdate: "tool_call", toolCallId: "call_perm", name: "bash", title: "Run: rm -rf build", kind: "execute", status: "in_progress" });
        update({ sessionUpdate: "tool_call_update", toolCallId: "call_perm", status: "completed", content: [{ type: "content", content: { type: "text", text: "build removed" } }] });
        respond(current.id, { stopReason: "end_turn" });
      } else {
        update({ sessionUpdate: "agent_message_chunk", messageId: "msg_cancelled", content: { type: "text", text: "Permission cancelled." } });
        respond(current.id, { stopReason: "cancelled" });
      }
      return;
    }

    case "cancel": {
      if (promptCount === 1) {
        const options = [
          { optionId: "allow-once", name: "Allow once", kind: "allow_once" },
          { optionId: "reject-once", name: "Reject", kind: "reject_once" },
        ];
        const answerPromise = askPermission(current, { toolCallId: "call_slow", title: "Run: a long command", kind: "execute", status: "pending" }, options);
        for (let index = 1; index <= 200; index += 1) {
          update({ sessionUpdate: "agent_message_chunk", messageId: "msg_slow", content: { type: "text", text: `chunk ${index} ` } });
          const state = await waitForCancel(delayMs, current);
          if (state === "cancelled") break;
        }
        const answer = await answerPromise;
        if (answer.outcome !== "cancelled") {
          die(5, "a cancelled turn must answer a pending permission with the `cancelled` outcome");
        }
        update({ sessionUpdate: "agent_message_chunk", messageId: "msg_cancelled", content: { type: "text", text: "Permission cancelled." } });
        respond(current.id, { stopReason: "cancelled" });
        return;
      }
      update({ sessionUpdate: "agent_message_chunk", messageId: "msg_after", content: { type: "text", text: `After the cancel: ${echo}` } });
      respond(current.id, { stopReason: "end_turn" });
      return;
    }

    case "unknown-line": {
      process.stdout.write("this is not json at all\n");
      process.stdout.write("\n");
      process.stdout.write("   \t \n");
      update({ sessionUpdate: "agent_message_chunk", messageId: "msg_1", content: { type: "text", text: "Still here: " } });
      notify("session/unknown_notification", { sessionId, payload: { future: true } });
      // A request the client never advertised support for: it must be answered (an error is fine)
      // rather than dropped, or a real agent would wait forever.
      write({ jsonrpc: "2.0", id: 77, method: "fs/read_text_file", params: { sessionId, path: "/tmp/from-the-future" } });
      const answered = awaitAnswer(77, "the unknown fs/read_text_file request");
      update({ sessionUpdate: "agent_message_chunk", messageId: "msg_1", content: { type: "text", text: echo } });
      update({ sessionUpdate: "plan", entries: [] });
      await answered;
      respond(current.id, { stopReason: "end_turn" });
      return;
    }

    case "crash": {
      update({ sessionUpdate: "agent_message_chunk", messageId: "msg_1", content: { type: "text", text: "Working on it..." } });
      await pause(30);
      // Die without answering the prompt: the client owns the `exited` transition.
      process.exit(7);
      return;
    }

    default:
      die(2, `mode ${mode} does not run prompt turns`);
  }
}

async function handlePrompt(message) {
  if (!initialized) die(2, "session/prompt before initialize");
  if (mode === "version-mismatch") die(3, "the client continued after a protocol-version mismatch");
  const params = message.params ?? {};
  if (params.sessionId !== sessionId) {
    die(2, `session/prompt for an unknown session: ${JSON.stringify(params.sessionId)}`);
  }
  const text = promptText(params);
  promptCount += 1;
  const current = { id: message.id, cancelled: false, onCancel: null, permissionId: null };
  turn = current;
  try {
    await runTurn(current, text);
  } finally {
    if (turn === current) turn = null;
  }
}

function handleCancel(message) {
  if (mode === "version-mismatch") die(3, "the client continued after a protocol-version mismatch");
  if (message.params?.sessionId !== sessionId) {
    process.stderr.write(`acp_fake: session/cancel for an unknown session: ${JSON.stringify(message.params?.sessionId)}\n`);
  }
  if (!turn) {
    process.stderr.write("acp_fake: session/cancel with no turn in flight\n");
    return;
  }
  turn.cancelled = true;
  if (turn.onCancel) turn.onCancel();
}

function handleRequest(message) {
  switch (message.method) {
    case "initialize":
      handleInitialize(message);
      return;
    case "session/new":
      handleNew(message);
      return;
    case "session/resume":
      handleResume(message);
      return;
    case "session/load":
      die(4, "session/load replays the whole conversation; a client must use session/resume");
      return;
    case "session/prompt":
      handlePrompt(message).catch((error) => die(1, `the turn failed: ${error?.stack ?? error}`));
      return;
    case "session/cancel":
      handleCancel(message);
      return;
    default:
      process.stderr.write(`acp_fake: unknown client method: ${message.method}\n`);
      if (message.id !== undefined && message.id !== null) {
        respondError(message.id, -32601, `acp_fake does not implement ${message.method}`);
      }
  }
}

function handleMessage(message) {
  if (message && typeof message.method === "string") {
    handleRequest(message);
    return;
  }
  handleResponse(message);
}

function handleLine(raw) {
  const line = raw.trim();
  if (line.length === 0) return;
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    process.stderr.write(`acp_fake: skipping a malformed line: ${line.slice(0, 120)}\n`);
    return;
  }
  handleMessage(message);
}

let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  let index = buffer.indexOf("\n");
  while (index >= 0) {
    const line = buffer.slice(0, index);
    buffer = buffer.slice(index + 1);
    handleLine(line);
    index = buffer.indexOf("\n");
  }
});
process.stdin.on("end", () => {
  // The client closed the pipe: nothing else can be asked of this process.
  if (!turn) process.exit(0);
});
