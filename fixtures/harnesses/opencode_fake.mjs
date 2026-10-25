#!/usr/bin/env node
// A scripted stand-in for `opencode serve --port <n> --hostname 127.0.0.1`.
//
// It never touches the network and never calls a model: it serves the v1 REST surface Grove's
// adapter uses (`/global/health`, `/session`, `/session/:id/prompt_async`, `/session/:id/abort`)
// plus the `/event` SSE bus, so adapter tests run offline and without a real `opencode` install.
//
// Modes (one fixture per behaviour the adapter must survive):
//
//   happy       text deltas, a tool call, an edit tool with a diff, `session.diff`, usage, idle
//   diff        `session.diff` entries with before/after *and* with an explicit `patch`
//   disconnect  the event stream is dropped mid-turn; the rest is replayed after the reconnect
//   permanent   the event stream is dropped and every reconnect is refused (a dead server)
//   slow        deltas with a delay, so a cancel has time to land mid-turn
//   cancel      same as slow (the name the ticket uses)
//   unknown     unknown, malformed and other-session frames interleaved with a valid answer
//   error       `session.error`, then idle
//   stalled     never listens and never exits; only writes stderr (a readiness timeout)
//
// Flags:
//   --port <n>           0 picks a free port; the chosen one is printed and written to --port-file
//   --hostname <host>    loopback only (the adapter must pass 127.0.0.1)
//   --state-dir <dir>    sessions live in <dir>/sessions.json, so a restarted server can be resumed
//   --ready-delay-ms <n> answer /global/health with 503 for n ms after binding (a slow start)
//   --heartbeat-file <f> append a line every --heartbeat-ms, so a test can prove the process died
//   --log-file <f>       JSONL of what the adapter actually did (requests, SSE connections)
//   --delay-ms <n>       pause between streamed events
//
// Everything the adapter is supposed to send is checked, not assumed: a missing `serve` subcommand,
// a non-loopback bind, or a prompt without a text part fails the fixture loudly.

import fs from "node:fs";
import http from "node:http";
import path from "node:path";

const argv = process.argv.slice(2);

function flag(name, fallback = null) {
  const index = argv.indexOf(name);
  return index >= 0 && index + 1 < argv.length ? argv[index + 1] : fallback;
}

const mode = flag("--mode", "happy");
const requestedPort = Number(flag("--port", "0"));
const hostname = flag("--hostname", "127.0.0.1");
const delayMs = Number(flag("--delay-ms", "20"));
const readyDelayMs = Number(flag("--ready-delay-ms", "0"));
let readyAt = 0;
const heartbeatFile = flag("--heartbeat-file", null);
const heartbeatMs = Number(flag("--heartbeat-ms", "100"));
const portFile = flag("--port-file", null);
const stateDir = flag("--state-dir", null);
const logFile = flag("--log-file", null);
const stderrReason = flag("--stderr", "opencode_fake: simulated startup failure");

function fail(reason) {
  process.stderr.write(`opencode_fake: ${reason}\n`);
  process.exit(2);
}

// The adapter must launch the documented subcommand and never bind outside loopback.
if (!argv.includes("serve")) fail("missing the `serve` subcommand");
if (!["127.0.0.1", "localhost", "::1", "[::1]"].includes(hostname)) {
  fail(`refusing to bind outside loopback: ${hostname}`);
}

function log(entry) {
  if (!logFile) return;
  try {
    fs.appendFileSync(logFile, `${JSON.stringify({ at: new Date().toISOString(), ...entry })}\n`);
  } catch {
    // Logging is observability, never a reason to break a turn.
  }
}

function heartbeat() {
  if (!heartbeatFile) return;
  try {
    fs.appendFileSync(heartbeatFile, `${Date.now()} ${process.pid}\n`);
  } catch {
    // Same: a missing heartbeat file must not take the fixture down.
  }
}

heartbeat();
setInterval(heartbeat, heartbeatMs).unref?.();

log({ event: "start", mode, pid: process.pid });

// --- session state ---------------------------------------------------------------------------

const sessions = new Map(); // id -> { id, title, created }
const stateFile = stateDir ? path.join(stateDir, "sessions.json") : null;

function loadState() {
  if (!stateFile || !fs.existsSync(stateFile)) return;
  try {
    const parsed = JSON.parse(fs.readFileSync(stateFile, "utf8"));
    for (const session of parsed.sessions ?? []) sessions.set(session.id, session);
  } catch (error) {
    process.stderr.write(`opencode_fake: ignoring unreadable state file: ${error}\n`);
  }
}

function saveState() {
  if (!stateFile) return;
  fs.mkdirSync(stateDir, { recursive: true });
  fs.writeFileSync(stateFile, JSON.stringify({ sessions: [...sessions.values()] }, null, 2));
}

function newSessionId() {
  return `ses_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`;
}

function createSession(title) {
  const now = Date.now();
  const session = { id: newSessionId(), projectID: "prj_fake", directory: process.cwd(), title, version: "fake-1.0.0", created: now, updated: now };
  sessions.set(session.id, session);
  saveState();
  log({ event: "session-created", sessionID: session.id, title });
  return session;
}

loadState();

// --- SSE -------------------------------------------------------------------------------------

const sseClients = new Set();
let refusingEvents = false;
let onNextConnect = null;

function broadcast(type, properties) {
  const frame = `data: ${JSON.stringify({ type, properties })}\n\n`;
  log({ event: "emit", type });
  for (const res of sseClients) {
    try {
      res.write(frame);
    } catch {
      sseClients.delete(res);
    }
  }
}

function dropClients() {
  log({ event: "sse-drop", clients: sseClients.size });
  for (const res of sseClients) {
    try {
      res.destroy();
    } catch {
      // already gone
    }
  }
  sseClients.clear();
}

function handleEventStream(req, res) {
  if (refusingEvents) {
    log({ event: "sse-refused" });
    res.writeHead(503, { "content-type": "application/json" });
    res.end(JSON.stringify({ name: "ServiceUnavailableError", data: { message: "event stream is dead" } }));
    return;
  }
  res.writeHead(200, {
    "content-type": "text/event-stream",
    "cache-control": "no-cache",
    connection: "keep-alive",
  });
  res.write(`data: ${JSON.stringify({ type: "server.connected", properties: {} })}\n\n`);
  sseClients.add(res);
  log({ event: "sse-connect", clients: sseClients.size });
  req.on("close", () => {
    sseClients.delete(res);
    log({ event: "sse-close", clients: sseClients.size });
  });
  const pending = onNextConnect;
  onNextConnect = null;
  if (pending) setTimeout(pending, 10);
}

// --- turns -----------------------------------------------------------------------------------

let turnSeq = 0;
const turns = new Map(); // sessionID -> { id, cancelled }
const turnCounts = new Map();

function newTurn(sessionID) {
  const id = ++turnSeq;
  turns.set(sessionID, { id, cancelled: false });
  turnCounts.set(sessionID, (turnCounts.get(sessionID) ?? 0) + 1);
  return { id, number: turnCounts.get(sessionID) };
}

function isCancelled(sessionID, turnId) {
  const current = turns.get(sessionID);
  return !current || current.id !== turnId || current.cancelled;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, Math.max(0, ms)));
}

function status(sessionID, value) {
  broadcast("session.status", { sessionID, status: value });
}

function message(sessionID, id, role) {
  broadcast("message.updated", { info: { id, sessionID, role, time: { created: Date.now() } } });
}

function part(part, delta) {
  const properties = { part };
  if (delta !== undefined) properties.delta = delta;
  broadcast("message.part.updated", properties);
}

function textPart(partId, sessionID, messageID, text, delta) {
  part({ id: partId, sessionID, messageID, type: "text", text }, delta);
}

function usagePart(sessionID, messageID, input, output) {
  part({
    id: `prt_step_${messageID}`,
    sessionID,
    messageID,
    type: "step-finish",
    reason: "stop",
    cost: 0,
    tokens: { input, output, reasoning: 0, cache: { read: 0, write: 0 } },
  });
}

function toolPart(sessionID, messageID, callID, tool, state) {
  part({ id: `prt_${callID}`, sessionID, messageID, type: "tool", callID, tool, state });
}

async function shortTurn(sessionID, prompt) {
  const { id } = newTurn(sessionID);
  status(sessionID, { type: "busy" });
  await sleep(delayMs);
  if (isCancelled(sessionID, id)) return;
  message(sessionID, "msg_short_user", "user");
  textPart("prt_short_user", sessionID, "msg_short_user", `ECHO: ${prompt}`);
  message(sessionID, "msg_short_assistant", "assistant");
  textPart("prt_short_assistant", sessionID, "msg_short_assistant", "Still here.", "Still here.");
  await sleep(delayMs);
  if (isCancelled(sessionID, id)) return;
  textPart("prt_short_assistant", sessionID, "msg_short_assistant", "Still here.");
  usagePart(sessionID, "msg_short_assistant", 5, 3);
  status(sessionID, { type: "idle" });
  turns.delete(sessionID);
}

async function slowTurn(sessionID, prompt) {
  const { id, number } = newTurn(sessionID);
  if (number > 1) return shortTurn(sessionID, prompt);
  status(sessionID, { type: "busy" });
  await sleep(delayMs);
  for (let index = 1; index <= 40; index += 1) {
    if (isCancelled(sessionID, id)) return;
    message(sessionID, "msg_slow", "assistant");
    textPart("prt_slow", sessionID, "msg_slow", `chunk ${index} `, `chunk ${index} `);
    await sleep(delayMs);
  }
  if (isCancelled(sessionID, id)) return;
  textPart("prt_slow", sessionID, "msg_slow", "A slow answer finished.");
  usagePart(sessionID, "msg_slow", 44, 21);
  status(sessionID, { type: "idle" });
  turns.delete(sessionID);
}

async function unknownTurn(sessionID, prompt) {
  const { id, number } = newTurn(sessionID);
  if (number > 1) return shortTurn(sessionID, prompt);
  status(sessionID, { type: "busy" });
  await sleep(delayMs);

  // Frames this adapter must log and drop: junk, unknown types, missing bodies, other sessions.
  for (const res of sseClients) {
    try {
      res.write("data: this is not json at all\n\n");
      res.write("event: message\ndata: {\"type\":\"from_the_future\",\"properties\":{}}\n\n");
      res.write(": keepalive comment\n\n");
      res.write("data: {}\n\n");
    } catch {
      sseClients.delete(res);
    }
  }
  broadcast("tui.prompt.append", { text: "not a transcript event" });
  broadcast("vcs.branch.updated", { branch: "main" });
  broadcast("session.status", { sessionID: "ses_other", status: { type: "idle" } });
  textPart("prt_other", "ses_other", "msg_other", "other session text", "other session text");
  part({ id: "prt_future", sessionID, messageID: "msg_unknown", type: "from-the-future" });

  // A user message part must never be rendered as an assistant turn.
  message(sessionID, "msg_unknown_user", "user");
  textPart("prt_unknown_user", sessionID, "msg_unknown_user", `ECHO: ${prompt}`);

  message(sessionID, "msg_unknown", "assistant");
  await sleep(delayMs);
  textPart("prt_unknown", sessionID, "msg_unknown", "Hel", "Hel");
  textPart("prt_unknown", sessionID, "msg_unknown", "Hello", "lo");
  await sleep(delayMs);
  if (isCancelled(sessionID, id)) return;
  textPart("prt_unknown", sessionID, "msg_unknown", "Hello");
  usagePart(sessionID, "msg_unknown", 3, 2);
  status(sessionID, { type: "idle" });
  turns.delete(sessionID);
}

async function happyTurn(sessionID, prompt, { includeSessionDiff = true } = {}) {
  const { id } = newTurn(sessionID);
  status(sessionID, { type: "busy" });
  await sleep(delayMs);

  message(sessionID, "msg_user_1", "user");
  textPart("prt_user_1", sessionID, "msg_user_1", `ECHO: ${prompt}`);
  message(sessionID, "msg_1", "assistant");

  for (const [partId, text, delta] of [
    ["prt_1", "Hello", "Hello"],
    ["prt_1", "Hello there", " there"],
  ]) {
    if (isCancelled(sessionID, id)) return;
    textPart(partId, sessionID, "msg_1", text, delta);
  }
  await sleep(delayMs);

  // A read tool: pending → completed, so the adapter must emit the call once and then its result.
  toolPart(sessionID, "msg_1", "call_read", "read", { status: "running", input: { filePath: "README.md" }, title: "Read README.md" });
  await sleep(delayMs);
  toolPart(sessionID, "msg_1", "call_read", "read", {
    status: "completed",
    input: { filePath: "README.md" },
    output: "# Grove\n\nOne desktop home for every AI coding harness.",
    title: "Read README.md",
    metadata: {},
    time: { start: Date.now() - 5, end: Date.now() },
  });

  // An edit tool: the patch rides on the state metadata, which is what a diff comes from.
  toolPart(sessionID, "msg_1", "call_edit", "edit", {
    status: "completed",
    input: { filePath: "src/main.rs" },
    output: "edited src/main.rs",
    title: "Edit src/main.rs",
    metadata: { diff: "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1,2 @@\n fn main() {}\n+    let answer = 42;\n" },
    time: { start: Date.now() - 4, end: Date.now() },
  });

  if (includeSessionDiff) {
    broadcast("session.diff", {
      sessionID,
      diff: [
        {
          file: "src/main.rs",
          before: "fn main() {}\n",
          after: "fn main() {}\n    let answer = 42;\n",
          additions: 1,
          deletions: 0,
        },
      ],
    });
  }

  textPart("prt_1", sessionID, "msg_1", "Hello there");
  usagePart(sessionID, "msg_1", 37, 12);
  await sleep(delayMs);
  if (isCancelled(sessionID, id)) return;
  status(sessionID, { type: "idle" });
  turns.delete(sessionID);
}

async function diffTurn(sessionID, prompt) {
  const { id } = newTurn(sessionID);
  status(sessionID, { type: "busy" });
  await sleep(delayMs);
  message(sessionID, "msg_diff_user", "user");
  textPart("prt_diff_user", sessionID, "msg_diff_user", `ECHO: ${prompt}`);
  message(sessionID, "msg_diff", "assistant");
  textPart("prt_diff", sessionID, "msg_diff", "Editing two files.", "Editing two files.");
  await sleep(delayMs);
  if (isCancelled(sessionID, id)) return;
  broadcast("session.diff", {
    sessionID,
    diff: [
      {
        file: "src/app.rs",
        before: "fn main() {\n    run();\n}\n",
        after: "fn main() {\n    run_twice();\n    run();\n}\n",
        additions: 1,
        deletions: 0,
      },
      {
        file: "docs/README.md",
        patch: "--- a/docs/README.md\n+++ b/docs/README.md\n@@ -1,2 +1,1 @@\n-# Grove\n-\n+# Grove\n",
        additions: 1,
        deletions: 2,
      },
    ],
  });
  toolPart(sessionID, "msg_diff", "call_diff_edit", "edit", {
    status: "completed",
    input: { filePath: "src/app.rs" },
    output: "edited src/app.rs",
    title: "Edit src/app.rs",
    metadata: { diff: "--- a/src/app.rs\n+++ b/src/app.rs\n@@ -1,3 +1,4 @@\n fn main() {\n+    run_twice();\n     run();\n }\n" },
  });
  usagePart(sessionID, "msg_diff", 19, 8);
  status(sessionID, { type: "idle" });
  turns.delete(sessionID);
}

async function disconnectTurn(sessionID, prompt) {
  const { id } = newTurn(sessionID);
  status(sessionID, { type: "busy" });
  await sleep(delayMs);
  message(sessionID, "msg_dc", "assistant");
  textPart("prt_dc", sessionID, "msg_dc", "Partial ", "Partial ");
  await sleep(delayMs);
  if (isCancelled(sessionID, id)) return;
  // Drop the stream mid-turn: the rest of the answer only exists for the reconnected client.
  onNextConnect = async () => {
    if (isCancelled(sessionID, id)) return;
    message(sessionID, "msg_dc", "assistant");
    textPart("prt_dc2", sessionID, "msg_dc", "and the rest.", "and the rest.");
    textPart("prt_dc2", sessionID, "msg_dc", "Partial and the rest.");
    usagePart(sessionID, "msg_dc", 21, 9);
    status(sessionID, { type: "idle" });
    turns.delete(sessionID);
  };
  dropClients();
}

async function permanentTurn(sessionID, prompt) {
  newTurn(sessionID);
  status(sessionID, { type: "busy" });
  await sleep(delayMs);
  message(sessionID, "msg_perm", "assistant");
  textPart("prt_perm", sessionID, "msg_perm", "Partial ", "Partial ");
  await sleep(delayMs);
  // Every reconnect attempt is refused from here on: this server is gone, not slow.
  refusingEvents = true;
  dropClients();
}

async function errorTurn(sessionID, prompt) {
  newTurn(sessionID);
  status(sessionID, { type: "busy" });
  await sleep(delayMs);
  broadcast("session.error", {
    sessionID,
    error: { name: "ApiError", data: { message: "the model provider is unavailable" } },
  });
  await sleep(delayMs);
  broadcast("session.idle", { sessionID });
  status(sessionID, { type: "idle" });
  turns.delete(sessionID);
}

async function runTurn(sessionID, prompt) {
  try {
    switch (mode) {
      case "happy":
        return await happyTurn(sessionID, prompt);
      case "diff":
        return await diffTurn(sessionID, prompt);
      case "disconnect":
        return await disconnectTurn(sessionID, prompt);
      case "permanent":
        return await permanentTurn(sessionID, prompt);
      case "slow":
      case "cancel":
        return await slowTurn(sessionID, prompt);
      case "unknown":
        return await unknownTurn(sessionID, prompt);
      case "error":
        return await errorTurn(sessionID, prompt);
      default:
        return await happyTurn(sessionID, prompt);
    }
  } catch (error) {
    process.stderr.write(`opencode_fake: turn failed: ${error?.stack ?? error}\n`);
  }
}

function abortTurn(sessionID) {
  const current = turns.get(sessionID);
  if (current) current.cancelled = true;
  log({ event: "abort", sessionID });
  status(sessionID, { type: "idle" });
  turns.delete(sessionID);
}

// --- HTTP ------------------------------------------------------------------------------------

function sendJson(res, statusCode, body) {
  const payload = JSON.stringify(body);
  res.writeHead(statusCode, { "content-type": "application/json", "content-length": Buffer.byteLength(payload) });
  res.end(payload);
}

function readBody(req) {
  return new Promise((resolve) => {
    const chunks = [];
    req.on("data", (chunk) => chunks.push(chunk));
    req.on("end", () => {
      const raw = Buffer.concat(chunks).toString("utf8");
      if (!raw.trim()) return resolve({});
      try {
        resolve(JSON.parse(raw));
      } catch {
        resolve(null);
      }
    });
  });
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, `http://${hostname}:${server.address()?.port ?? requestedPort}`);
  log({ event: "request", method: req.method, path: url.pathname });

  if (req.method === "GET" && url.pathname === "/global/health") {
    // `--ready-delay-ms` answers 503 while the (already bound) server warms up, which proves the
    // adapter polls readiness instead of testing the port once and giving up. Binding immediately
    // keeps the port reserved for this fixture, so no other process can answer in its place.
    if (Date.now() < readyAt) {
      log({ event: "health-not-ready" });
      return sendJson(res, 503, { healthy: false, version: "fake-1.0.0" });
    }
    return sendJson(res, 200, { healthy: true, version: "fake-1.0.0" });
  }

  if (req.method === "GET" && url.pathname === "/event") {
    return handleEventStream(req, res);
  }

  if (req.method === "POST" && url.pathname === "/session") {
    const body = await readBody(req);
    if (body === null) return sendJson(res, 400, { name: "BadRequestError", data: { message: "invalid JSON" } });
    const session = createSession(body.title ?? "grove session");
    return sendJson(res, 200, {
      id: session.id,
      projectID: session.projectID,
      directory: session.directory,
      title: session.title,
      version: session.version,
      time: { created: session.created, updated: session.updated },
    });
  }

  const sessionMatch = url.pathname.match(/^\/session\/([^/]+)(?:\/(prompt_async|abort|message))?$/);
  if (sessionMatch) {
    const [, sessionID, action] = sessionMatch;
    const session = sessions.get(sessionID);
    if (!session) {
      return sendJson(res, 404, { name: "NotFoundError", data: { message: `session ${sessionID} not found` } });
    }

    if (req.method === "GET" && !action) {
      return sendJson(res, 200, {
        id: session.id,
        projectID: session.projectID,
        directory: session.directory,
        title: session.title,
        version: session.version,
        time: { created: session.created, updated: session.updated },
      });
    }

    if (req.method === "POST" && action === "prompt_async") {
      const body = await readBody(req);
      if (body === null) return sendJson(res, 400, { name: "BadRequestError", data: { message: "invalid JSON" } });
      const text = (body.parts ?? []).find((candidate) => candidate?.type === "text")?.text;
      if (typeof text !== "string" || text.length === 0) {
        log({ event: "prompt-rejected", sessionID });
        return sendJson(res, 400, { name: "BadRequestError", data: { message: "a text part is required" } });
      }
      log({ event: "prompt", sessionID, text, model: body.model ?? null });
      res.writeHead(204);
      res.end();
      setTimeout(() => void runTurn(sessionID, text), 5);
      return undefined;
    }

    if (req.method === "POST" && action === "abort") {
      abortTurn(sessionID);
      return sendJson(res, 200, true);
    }

    if (req.method === "GET" && action === "message") {
      return sendJson(res, 200, []);
    }

    return sendJson(res, 405, { name: "MethodNotAllowedError", data: { message: `${req.method} ${url.pathname}` } });
  }

  return sendJson(res, 404, { name: "NotFoundError", data: { message: url.pathname } });
});

if (mode === "stalled") {
  // Never listen and never exit: only a readiness timeout (with this stderr) can end the session.
  process.stderr.write(`${stderrReason}\n`);
  process.stderr.write("opencode_fake: the vault is locked, refusing to bind\n");
  setInterval(() => {}, 1000);
} else {
  server.listen(requestedPort, hostname, () => {
    const chosen = server.address().port;
    // The warm-up clock starts when the socket is actually serving, so a slow Node start can never
    // swallow the simulated readiness delay.
    readyAt = Date.now() + readyDelayMs;
    log({ event: "listening", port: chosen, hostname });
    if (portFile) fs.writeFileSync(portFile, String(chosen));
    process.stdout.write(`opencode server listening on http://${hostname}:${chosen}\n`);
  });
  server.on("error", (error) => {
    process.stderr.write(`opencode_fake: listen failed: ${error}\n`);
    process.exit(3);
  });
}

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    log({ event: "exit", signal });
    process.exit(0);
  });
}
