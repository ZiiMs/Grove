#!/usr/bin/env node
// A scripted stand-in for the Grove Node sidecar (`sidecar/dist/main.js`).
//
// It speaks **Grove Sidecar Protocol v1** (NDJSON JSON-RPC 2.0 on stdio — see
// `docs/sidecar-protocol.md`) and replays already-normalized `HarnessEvent`s, so the Rust `pi`
// adapter can be tested without Node's sidecar build, without pi, and without the network. The real
// sidecar is the thing that understands pi; this fixture only proves the adapter's side of the
// handshake, the forwarding, and the failure modes.
//
//   node pi_sidecar_fake.mjs --mode <happy|tools|slow|unknown|crash|unavailable|handshake>
//                            [--delay-ms N] [--heartbeat <file>]
//
//   happy        session.start → session_started/ready, then busy, deltas, message, usage, idle
//   tools        the same turn with a tool_call/tool_result pair before the answer
//   slow         deltas with a delay, so a cancel lands mid-turn
//   unknown      junk lines, unknown notifications and unparseable events around a valid turn
//   crash        a delta and then the sidecar dies mid-turn (no idle)
//   unavailable  initialize reports pi as unavailable and session.start fails with -32000
//   handshake    the sidecar dies before answering initialize (a broken build, a bad node)
//
// Everything is validated: a missing `initialize`, a `session.start` without a session id, or the
// wrong protocol version exits non-zero, so the adapter's handshake is tested rather than assumed.

import { appendFileSync } from "node:fs";

const argv = process.argv.slice(2);

function flag(name, fallback = null) {
  const index = argv.indexOf(name);
  return index >= 0 && index + 1 < argv.length ? argv[index + 1] : fallback;
}

const mode = flag("--mode", "happy");
const delayMs = Number(flag("--delay-ms", "0"));
const heartbeat = flag("--heartbeat", null);
const PROTOCOL_VERSION = 1;

if (!["happy", "tools", "slow", "unknown", "crash", "unavailable", "handshake"].includes(mode)) {
  process.stderr.write(`pi_sidecar_fake: unknown mode: ${mode}\n`);
  process.exit(2);
}

let sessionId = null;
let resumedToken = null;
let stopped = false;
let aborted = false;
let turn = 0;

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function respond(id, result) {
  write({ jsonrpc: "2.0", id, result });
}

function respondError(id, code, message, data) {
  write({ jsonrpc: "2.0", id, error: { code, message, data } });
}

function event(kind) {
  write({
    jsonrpc: "2.0",
    method: "session.event",
    params: { sessionId, event: { at: new Date().toISOString(), ...kind } },
  });
}

function status(state) {
  event({ kind: "status", state });
}

function pause(ms = delayMs) {
  return ms > 0 ? new Promise((resolve) => setTimeout(resolve, ms)) : Promise.resolve();
}

if (heartbeat) {
  // Used by the orphan test: this file stops growing when the sidecar process dies.
  appendFileSync(heartbeat, `${process.pid}\n`);
  setInterval(() => appendFileSync(heartbeat, "beat\n"), 50);
}

if (mode === "handshake") {
  process.stderr.write("pi_sidecar_fake: no handshake today\n");
  process.exit(3);
}

function token() {
  return `C:/fake-pi/sessions/session-${process.pid}.jsonl`;
}

async function runTurn(text) {
  turn += 1;
  aborted = false;
  status("busy");

  if (mode === "crash") {
    event({ kind: "delta", text: "half an ans" });
    await pause(30);
    process.exit(7);
  }

  if (mode === "unknown") {
    // Not JSON at all, a JSON array, an unknown notification method, an event kind Rust does not
    // know, and an event missing every required field. All of it must be logged and dropped.
    process.stdout.write("this is not json\n");
    process.stdout.write("\n");
    write([1, 2, 3]);
    write({ jsonrpc: "2.0", method: "session.progress", params: { sessionId, done: 0.5 } });
    write({
      jsonrpc: "2.0",
      method: "session.event",
      params: { sessionId, event: { at: new Date().toISOString(), kind: "from_the_future" } },
    });
    write({ jsonrpc: "2.0", method: "session.event", params: { sessionId, event: { kind: "delta" } } });
    write({ jsonrpc: "2.0", method: "session.event", params: { sessionId: "someone-else", event: {} } });
    await pause();
  }

  if (mode === "tools") {
    event({ kind: "tool_call", call_id: "call_1", name: "bash", title: "ls -la" });
    await pause();
    event({ kind: "tool_result", call_id: "call_1", status: "ok", summary: "total 0\nREADME.md" });
    await pause();
    event({ kind: "tool_call", call_id: "call_2", name: "read", title: "C:/work/grove/README.md" });
    await pause();
    event({ kind: "tool_result", call_id: "call_2", status: "error", summary: "error: no such file" });
    await pause();
  }

  const answer = `Fake reply: ${text}`;
  // Only the first slow turn is long: a later turn proves the session still works after a cancel.
  const chunks =
    mode === "slow" && turn === 1
      ? Array.from({ length: 30 }, (_, index) => `chunk ${index + 1} `)
      : answer.split(/(?= )/u);
  for (const chunk of chunks) {
    if (aborted || stopped) return;
    event({ kind: "delta", text: chunk });
    await pause();
  }
  if (aborted || stopped) return;

  event({
    kind: "message",
    role: "assistant",
    blocks: [
      { type: "thinking", text: "Answer briefly." },
      { type: "text", text: answer },
    ],
  });
  event({ kind: "usage", input_tokens: 42 * turn, output_tokens: 11 });
  await pause();
  if (!aborted && !stopped) status("idle");
}

function handle(message) {
  if (!message || typeof message !== "object" || message.jsonrpc !== "2.0") {
    process.stderr.write("pi_sidecar_fake: not a JSON-RPC 2.0 message\n");
    process.exit(4);
  }
  const { id, method, params } = message;

  switch (method) {
    case "initialize": {
      if (params?.protocolVersion !== PROTOCOL_VERSION) {
        process.stderr.write(`pi_sidecar_fake: unexpected protocolVersion: ${params?.protocolVersion}\n`);
        process.exit(4);
      }
      if (mode === "unavailable") {
        respond(id, {
          protocolVersion: PROTOCOL_VERSION,
          version: "fake",
          harnesses: ["pi"],
          pi: { available: false, reason: "pi is not installed or not on PATH (looked for `pi`)" },
        });
        return;
      }
      respond(id, {
        protocolVersion: PROTOCOL_VERSION,
        version: "fake",
        harnesses: ["pi"],
        pi: { available: true, command: "pi" },
      });
      return;
    }
    case "session.start": {
      if (typeof params?.sessionId !== "string" || params.sessionId.length === 0) {
        process.stderr.write("pi_sidecar_fake: session.start without a sessionId\n");
        process.exit(4);
      }
      if (typeof params?.cwd !== "string" || params.cwd.length === 0) {
        process.stderr.write("pi_sidecar_fake: session.start without a cwd\n");
        process.exit(4);
      }
      sessionId = params.sessionId;
      resumedToken = params.resumeToken ?? null;
      if (mode === "unavailable") {
        respondError(id, -32000, "pi is not installed or not on PATH (looked for `pi`)", {
          reason: "pi is not installed or not on PATH (looked for `pi`)",
        });
        return;
      }
      const resumeToken = resumedToken ?? token();
      event({
        kind: "session_started",
        harness: "pi",
        model: params.model ?? null,
        resumed: Boolean(resumedToken),
      });
      status("starting");
      status("ready");
      respond(id, { resumeToken });
      const first = typeof params.prompt === "string" ? params.prompt : null;
      if (first) setImmediate(() => void runTurn(first));
      return;
    }
    case "session.send": {
      if (params?.sessionId !== sessionId) {
        process.stderr.write("pi_sidecar_fake: session.send for an unknown session\n");
        process.exit(4);
      }
      respond(id, {});
      void runTurn(params.text);
      return;
    }
    case "session.cancel": {
      aborted = true;
      respond(id, {});
      status("idle");
      return;
    }
    case "session.permission": {
      respondError(id, -32001, "pi has no tool-approval gate");
      return;
    }
    case "session.stop": {
      stopped = true;
      respond(id, {});
      process.exit(0);
      return;
    }
    default: {
      respondError(id, -32601, `unknown method: ${method}`);
    }
  }
}

// LF-only framing: split on "\n", tolerate a preceding CR. Never `readline` (U+2028/29 are legal in
// JSON strings).
let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  let index = buffer.indexOf("\n");
  while (index >= 0) {
    const line = buffer.slice(0, index).replace(/\r$/, "");
    buffer = buffer.slice(index + 1);
    if (line.trim().length > 0) {
      try {
        handle(JSON.parse(line));
      } catch (error) {
        process.stderr.write(`pi_sidecar_fake: bad request line: ${error.message}\n`);
        process.exit(4);
      }
    }
    index = buffer.indexOf("\n");
  }
});

process.stdin.on("end", () => process.exit(0));
