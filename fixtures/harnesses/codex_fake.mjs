#!/usr/bin/env node
// A scripted stand-in for `codex app-server`: JSON-RPC 2.0 over newline-delimited stdio, with the
// `"jsonrpc":"2.0"` header omitted on the wire exactly like the real server.
//
// It never touches the network and never calls a model: it replays the v2 thread/turn shapes
// documented for the Codex app-server so the adapter tests run offline and without a real `codex`
// install. One process serves the whole conversation, so every mode is a turn script:
//
//   happy         a command execution, streamed answer, aggregated turn diff, token usage
//   approval      a server -> client approval request the adapter has to answer
//   diff          an aggregated turn diff (sent twice, so the adapter must de-dupe) plus usage
//   error         the first turn fails with a structured JSON-RPC error, later turns work
//   crash         output, then the process dies mid-turn without `turn/completed`
//   unknown-line  junk, unknown notifications and unknown responses around a valid answer
//   slow          answer deltas with a delay, so a cancel has time to land mid-stream
//
// The fixture is the oracle for the launch form and the request shapes: it exits non-zero when the
// adapter does not pass `app-server`, when `initialize` carries a `protocolVersion` (app-server has
// none), when a request arrives before the `initialized` notification, or when `turn/start` input is
// not `[{type: "text", text, text_elements: []}]`.
//
// `thread/resume` echoes the id it was given, which is what makes the resume-token roundtrip
// observable; a `threadId` starting with `expired-` is refused so the adapter's fallback is testable.
//
// `CODEX_FAKE_LOG=<path>` appends a protocol trace (received frames, emitted notifications) for
// debugging a session without a real `codex` install. It is off unless the variable is set.

import fs from "node:fs";

const argv = process.argv.slice(2);

function flag(name, fallback = null) {
  const index = argv.indexOf(name);
  return index >= 0 && index + 1 < argv.length ? argv[index + 1] : fallback;
}

const mode = flag("--mode", "happy");
const delayMs = Number(flag("--delay-ms", "0"));

function fatal(message) {
  process.stderr.write(`codex_fake: ${message}\n`);
  process.exit(2);
}

// `codex app-server` is the documented launch form; without it nothing else is real.
if (!argv.includes("app-server")) {
  fatal(`missing the app-server subcommand; got: ${JSON.stringify(argv)}`);
}

const FAKE_DIFF = [
  "diff --git a/crates/grove-core/src/lib.rs b/crates/grove-core/src/lib.rs",
  "--- a/crates/grove-core/src/lib.rs",
  "+++ b/crates/grove-core/src/lib.rs",
  "@@ -1,3 +1,4 @@",
  " pub mod adapters;",
  "+pub mod codex;",
  " pub mod ipc;",
  " pub mod paths;",
  "",
].join("\n");

let threadId = `fake-thread-${process.pid}`;
let initializeSeen = false;
let initializedNotification = false;
let turnCount = 0;
let currentTurn = null;
/** Server -> client requests we are waiting on: `${id}` -> resolve(message). */
const awaitingReply = new Map();

function writeLine(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function notify(method, params) {
  trace(`emit ${method} ${JSON.stringify(params?.turnId ?? params?.turn?.id ?? "")}`);
  writeLine({ method, params });
}

/** Opt-in protocol trace (`CODEX_FAKE_LOG=<path>`), used to debug the adapter without a real CLI. */
function trace(line) {
  const path = process.env.CODEX_FAKE_LOG;
  if (!path) return;
  try {
    fs.appendFileSync(path, `${line}
`);
  } catch {
    /* tracing must never break a session */
  }
}

function reply(id, result) {
  writeLine({ id, result });
}

function replyError(id, code, message) {
  writeLine({ id, error: { code, message } });
}

function request(id, method, params) {
  writeLine({ id, method, params });
}

class Cancelled extends Error {}

async function tick(turn, ms = delayMs) {
  if (ms > 0) await new Promise((resolve) => setTimeout(resolve, ms));
  if (turn.cancelled) throw new Cancelled();
}

function firstLine(text) {
  return (
    String(text)
      .split(/\r?\n/)
      .find((line) => line.trim().length > 0)
      ?.trim() ?? ""
  );
}

/** Wait for the client's answer to a server -> client request without stalling the stdin loop. */
function waitForReply(id) {
  return new Promise((resolve, reject) => {
    const key = String(id);
    const timer = setTimeout(() => {
      awaitingReply.delete(key);
      reject(new Error(`the client never answered server request ${key}`));
    }, 15000);
    awaitingReply.set(key, (message) => {
      clearTimeout(timer);
      resolve(message);
    });
  });
}

function threadParams(params) {
  if (typeof params?.cwd !== "string" || params.cwd.length === 0) fatal("thread/start is missing cwd");
}

function turnInput(params) {
  const input = params?.input;
  if (!Array.isArray(input) || input.length === 0) fatal("turn/start is missing input");
  const first = input[0];
  if (first?.type !== "text" || typeof first.text !== "string" || !Array.isArray(first.text_elements)) {
    fatal(`turn/start input must be [{type:"text", text, text_elements: []}], got ${JSON.stringify(input)}`);
  }
  return first.text;
}

async function runHappyTurn(turn, echo) {
  notify("turn/started", { threadId, turn: { id: turn.id } });
  await tick(turn);
  notify("item/started", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1712,
    item: {
      type: "commandExecution",
      id: "item_exec_1",
      command: "cargo test -p grove-core",
      cwd: process.cwd(),
      status: "inProgress",
      aggregatedOutput: null,
      exitCode: null,
      durationMs: null,
    },
  });
  await tick(turn);
  notify("item/commandExecution/outputDelta", {
    threadId,
    turnId: turn.id,
    itemId: "item_exec_1",
    delta: "running 24 tests\n",
  });
  await tick(turn);
  notify("item/completed", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1712,
    item: {
      type: "commandExecution",
      id: "item_exec_1",
      command: "cargo test -p grove-core",
      cwd: process.cwd(),
      status: "completed",
      aggregatedOutput: "running 24 tests\nall 24 passed",
      exitCode: 0,
      durationMs: 1430,
    },
  });
  await tick(turn);
  notify("item/started", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1713,
    item: { type: "agentMessage", id: "item_msg_1", text: "", phase: null },
  });
  notify("item/agentMessage/delta", {
    threadId,
    turnId: turn.id,
    itemId: "item_msg_1",
    delta: "Fake reply: ",
  });
  await tick(turn);
  notify("item/agentMessage/delta", {
    threadId,
    turnId: turn.id,
    itemId: "item_msg_1",
    delta: echo,
  });
  await tick(turn);
  notify("item/completed", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1713,
    item: { type: "agentMessage", id: "item_msg_1", text: `Fake reply: ${echo}`, phase: null },
  });
  await tick(turn);
  notify("turn/diff/updated", { threadId, turnId: turn.id, diff: FAKE_DIFF });
  await tick(turn);
  notify("thread/tokenUsage/updated", {
    threadId,
    turnId: turn.id,
    tokenUsage: {
      total: { input_tokens: 40, output_tokens: 15, total_tokens: 55 },
      last: { input_tokens: 40, output_tokens: 15, total_tokens: 55 },
      modelContextWindow: 272000,
    },
  });
  await tick(turn);
  notify("turn/completed", { threadId, turn: { id: turn.id, status: "completed" } });
}

async function runApprovalTurn(turn, echo) {
  notify("turn/started", { threadId, turn: { id: turn.id } });
  await tick(turn);
  notify("item/started", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1712,
    item: {
      type: "commandExecution",
      id: "item_exec_1",
      command: "rm -rf build",
      cwd: process.cwd(),
      status: "inProgress",
      aggregatedOutput: null,
      exitCode: null,
      durationMs: null,
    },
  });
  await tick(turn);

  // The approval channel: a server -> client *request* (it carries an id and expects a result).
  const approvalId = 1000 + turnCount;
  request(approvalId, "item/commandExecution/requestApproval", {
    threadId,
    turnId: turn.id,
    itemId: "item_exec_1",
    command: "rm -rf build",
    cwd: process.cwd(),
    reason: "deletes the build directory",
  });
  const answer = await waitForReply(approvalId);
  const decision = answer?.result?.decision;
  if (decision !== "accept" && decision !== "decline" && decision !== "cancel") {
    fatal(
      `the approval reply must be {"decision":"accept"|"decline"|"cancel"}, got ${JSON.stringify(answer)}`,
    );
  }
  if (turn.cancelled) throw new Cancelled();
  const approved = decision === "accept";

  notify("item/completed", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1712,
    item: {
      type: "commandExecution",
      id: "item_exec_1",
      command: "rm -rf build",
      cwd: process.cwd(),
      status: approved ? "completed" : "failed",
      aggregatedOutput: `${approved ? "approved" : "denied"}: rm -rf build`,
      exitCode: approved ? 0 : 1,
      durationMs: 40,
    },
  });
  await tick(turn);
  notify("item/agentMessage/delta", {
    threadId,
    turnId: turn.id,
    itemId: "item_msg_1",
    delta: `Approval ${decision}.`,
  });
  await tick(turn);
  notify("item/completed", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1713,
    item: {
      type: "agentMessage",
      id: "item_msg_1",
      text: `Approval ${decision} for: ${echo}`,
      phase: null,
    },
  });
  await tick(turn);
  notify("turn/completed", { threadId, turn: { id: turn.id, status: "completed" } });
}

async function runDiffTurn(turn, echo) {
  notify("turn/started", { threadId, turn: { id: turn.id } });
  await tick(turn);
  notify("turn/diff/updated", { threadId, turnId: turn.id, diff: FAKE_DIFF });
  await tick(turn);
  // The server re-sends the aggregated diff as more files land; the adapter must not double-render it.
  notify("turn/diff/updated", { threadId, turnId: turn.id, diff: FAKE_DIFF });
  await tick(turn);
  notify("item/agentMessage/delta", {
    threadId,
    turnId: turn.id,
    itemId: "item_msg_1",
    delta: `Diff for: ${echo}`,
  });
  await tick(turn);
  notify("item/completed", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1713,
    item: { type: "agentMessage", id: "item_msg_1", text: `Diff for: ${echo}`, phase: null },
  });
  await tick(turn);
  notify("thread/tokenUsage/updated", {
    threadId,
    turnId: turn.id,
    tokenUsage: { total: { input_tokens: 12, output_tokens: 3 }, last: { input_tokens: 12, output_tokens: 3 } },
  });
  await tick(turn);
  notify("turn/completed", { threadId, turn: { id: turn.id, status: "completed" } });
}

async function runRecoveredTurn(turn, echo) {
  notify("turn/started", { threadId, turn: { id: turn.id } });
  await tick(turn);
  notify("item/agentMessage/delta", {
    threadId,
    turnId: turn.id,
    itemId: "item_msg_1",
    delta: `Recovered: ${echo}`,
  });
  await tick(turn);
  notify("item/completed", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1713,
    item: { type: "agentMessage", id: "item_msg_1", text: `Recovered: ${echo}`, phase: null },
  });
  await tick(turn);
  notify("turn/completed", { threadId, turn: { id: turn.id, status: "completed" } });
}

async function runCrashTurn(turn, echo) {
  notify("turn/started", { threadId, turn: { id: turn.id } });
  await new Promise((resolve) => setTimeout(resolve, 10));
  notify("item/agentMessage/delta", {
    threadId,
    turnId: turn.id,
    itemId: "item_msg_1",
    delta: `Working on: ${echo}`,
  });
  // Give the pipe a moment to flush, then die without `turn/completed`.
  await new Promise((resolve) => setTimeout(resolve, 50));
  process.exit(9);
}

async function runUnknownTurn(turn) {
  // A known-but-unused notification, junk lines, an unknown method, an array, an unknown response id
  // and a reasoning delta: every one of these must be a log line, never a dead session.
  notify("thread/status/changed", { threadId, status: { type: "active" } });
  process.stdout.write("this is not json at all\n");
  process.stdout.write("\n");
  process.stdout.write("   \t\n");
  process.stdout.write(`${JSON.stringify([1, 2, 3])}\n`);
  notify("from/the/future", { threadId, payload: { anything: true } });
  process.stdout.write(`${JSON.stringify({ id: 987654, result: { unexpected: true } })}\n`);
  notify("item/reasoning/summaryTextDelta", {
    threadId,
    turnId: turn.id,
    itemId: "item_reason_1",
    delta: "thinking about it",
  });
  notify("item/started", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1712,
    item: { type: "reasoning", id: "item_reason_1", summary: [], content: [] },
  });
  await tick(turn);
  notify("item/agentMessage/delta", {
    threadId,
    turnId: turn.id,
    itemId: "item_msg_1",
    delta: "Hello from ",
  });
  await tick(turn);
  notify("item/agentMessage/delta", {
    threadId,
    turnId: turn.id,
    itemId: "item_msg_1",
    delta: "the fixture.",
  });
  await tick(turn);
  notify("item/completed", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1713,
    item: { type: "agentMessage", id: "item_msg_1", text: "Hello from the fixture.", phase: null },
  });
  emitFutureNotification();
  await tick(turn);
  notify("turn/completed", { threadId, turn: { id: turn.id, status: "completed" } });
}

/** A notification shape no adapter has heard of yet, sent as a raw line to keep it future-proof. */
function emitFutureNotification() {
  process.stdout.write(
    `${JSON.stringify({ method: "hook/started", params: { threadId, hook: { id: "hook_1" } } })}\n`,
  );
}

async function runSlowTurn(turn, echo) {
  notify("turn/started", { threadId, turn: { id: turn.id } });
  await tick(turn);
  for (let index = 1; index <= 40; index += 1) {
    notify("item/agentMessage/delta", {
      threadId,
      turnId: turn.id,
      itemId: "item_msg_1",
      delta: `chunk ${index} `,
    });
    await tick(turn, Math.max(delayMs, 1));
  }
  notify("item/completed", {
    threadId,
    turnId: turn.id,
    startedAtMs: 1713,
    item: { type: "agentMessage", id: "item_msg_1", text: `A slow answer finished: ${echo}`, phase: null },
  });
  notify("turn/completed", { threadId, turn: { id: turn.id, status: "completed" } });
}

function runTurn(turn, promptText) {
  const echo = firstLine(promptText);
  const script = {
    happy: runHappyTurn,
    approval: runApprovalTurn,
    diff: runDiffTurn,
    "unknown-line": (current) => runUnknownTurn(current),
    slow: runSlowTurn,
    crash: runCrashTurn,
    error: runRecoveredTurn,
  }[mode];

  if (!script) fatal(`unknown mode: ${mode}`);

  return script(turn, echo).catch((error) => {
    if (error instanceof Cancelled) return;
    process.stderr.write(`codex_fake: turn failed: ${error?.stack ?? error}\n`);
    process.exit(3);
  });
}

function handleRequest(id, method, params) {
  if (method === "initialize") {
    if (params && Object.prototype.hasOwnProperty.call(params, "protocolVersion")) {
      fatal("initialize carried a protocolVersion field; app-server has no protocol version");
    }
    if (typeof params?.clientInfo?.name !== "string" || params.clientInfo.name.length === 0) {
      fatal(`initialize is missing clientInfo.name: ${JSON.stringify(params)}`);
    }
    initializeSeen = true;
    reply(id, { userAgent: "codex_fake/0.1.0", platformFamily: "fake", platformOs: "fake" });
    return;
  }
  if (!initializeSeen || !initializedNotification) {
    // The handshake is mandatory and once per connection, otherwise the real server says
    // `Not initialized`.
    replyError(id, -32002, "Not initialized");
    return;
  }

  switch (method) {
    case "thread/start": {
      threadParams(params);
      threadId = `fake-thread-${process.pid}`;
      reply(id, { thread: { id: threadId } });
      notify("thread/started", { thread: { id: threadId } });
      return;
    }

    case "thread/resume": {
      if (typeof params?.threadId !== "string" || params.threadId.length === 0) {
        fatal(`thread/resume is missing threadId: ${JSON.stringify(params)}`);
      }
      if (params.threadId.startsWith("expired-")) {
        replyError(id, -32600, `no rollout found for thread id ${params.threadId}`);
        return;
      }
      threadId = params.threadId;
      reply(id, { thread: { id: threadId } });
      notify("thread/started", { thread: { id: threadId } });
      return;
    }

    case "turn/start": {
      if (params?.threadId !== threadId) {
        replyError(id, -32602, `unknown thread: ${JSON.stringify(params?.threadId)}`);
        return;
      }
      const text = turnInput(params);
      turnCount += 1;
      if (mode === "error" && turnCount === 1) {
        // A structured turn error: reported, and the session has to survive it.
        replyError(id, -32000, "model request failed: 529 overloaded");
        return;
      }
      const turn = { id: `turn_${turnCount}`, cancelled: false };
      currentTurn = turn;
      reply(id, { turn: { id: turn.id } });
      void runTurn(turn, text);
      return;
    }

    case "turn/interrupt": {
      if (params?.threadId !== threadId) {
        replyError(id, -32602, `unknown thread: ${JSON.stringify(params?.threadId)}`);
        return;
      }
      if (!currentTurn || currentTurn.id !== params.turnId) {
        replyError(id, -32001, `no active turn ${JSON.stringify(params?.turnId)}`);
        return;
      }
      const turn = currentTurn;
      currentTurn = null;
      turn.cancelled = true;
      reply(id, {});
      notify("turn/completed", { threadId, turn: { id: turn.id, status: "interrupted" } });
      return;
    }

    default:
      replyError(id, -32601, `method not found: ${method}`);
  }
}

function handleLine(raw) {
  if (raw.trim().length === 0) return;
  let message;
  try {
    message = JSON.parse(raw);
  } catch {
    fatal(`the client wrote a line that is not JSON: ${raw.slice(0, 200)}`);
  }
  if (message === null || typeof message !== "object" || Array.isArray(message)) {
    fatal(`the client wrote a JSON value that is not an object: ${raw.slice(0, 200)}`);
  }

  const { id, method } = message;
  trace(`read ${JSON.stringify({ id, method, turnCount, cancelled: currentTurn?.cancelled })}`);
  if (typeof method === "string") {
    if (id === undefined) {
      if (method === "initialized") {
        initializedNotification = true;
      } else {
        process.stderr.write(`codex_fake: ignoring unknown notification ${method}\n`);
      }
      return;
    }
    handleRequest(id, method, message.params ?? {});
    return;
  }

  if (id !== undefined) {
    const resolver = awaitingReply.get(String(id));
    if (!resolver) {
      process.stderr.write(`codex_fake: response for an unknown id: ${raw.slice(0, 200)}\n`);
      return;
    }
    awaitingReply.delete(String(id));
    resolver(message);
    return;
  }

  fatal(`the client wrote a message the fixture cannot classify: ${raw.slice(0, 200)}`);
}

let buffered = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffered += chunk;
  let index = buffered.indexOf("\n");
  while (index >= 0) {
    const raw = buffered.slice(0, index).replace(/\r$/, "");
    buffered = buffered.slice(index + 1);
    try {
      handleLine(raw);
    } catch (error) {
      process.stderr.write(`codex_fake: ${error?.stack ?? error}\n`);
      process.exit(3);
    }
    index = buffered.indexOf("\n");
  }
});

// The client closing stdin means the session is over.
process.stdin.on("end", () => process.exit(0));
