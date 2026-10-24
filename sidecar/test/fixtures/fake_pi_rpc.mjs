#!/usr/bin/env node
// A scripted stand-in for `pi --mode rpc`, used by the sidecar's own tests.
//
// It never touches the network and never calls a model: it replays the newline-delimited record
// shapes documented for pi's RPC mode (see `C:/Users/zyko8/.grove-notes/protocols.md` §5), so the
// sidecar's mapping runs offline and without a real `pi` install.
//
// The scenario comes from `--scenario`, because `--mode rpc` is pi's own (required) flag:
//
//   node fake_pi_rpc.mjs --mode rpc --scenario <happy|tools|slow|unknown|crash> [--delay-ms N]
//                        [--session-file <path>] [--heartbeat <file>]
//
//   happy    text deltas, message_end, usage, agent_settled
//   tools    a tool_execution_start/end pair around the answer
//   slow     deltas with a delay, so a cancel lands mid-turn
//   unknown  junk, unknown record types and extension UI traffic around a valid answer
//   crash    some output, then the process dies mid-turn (no agent_settled)
//
// Framing is strict JSONL split on LF only — the same rule pi documents for its own stdout, so the
// fake cannot accidentally forgive a reader that splits on U+2028/29.

import { appendFileSync } from "node:fs";

const argv = process.argv.slice(2);

function flag(name, fallback = null) {
  const index = argv.indexOf(name);
  return index >= 0 && index + 1 < argv.length ? argv[index + 1] : fallback;
}

// pi's RPC mode is `--mode rpc`; failing here proves the sidecar passes the documented flag.
if (flag("--mode") !== "rpc") {
  process.stderr.write("fake_pi_rpc: missing required flag: --mode rpc\n");
  process.exit(2);
}

const scenario = flag("--scenario", "happy");
const delayMs = Number(flag("--delay-ms", "0"));
const heartbeat = flag("--heartbeat", null);
let sessionFile = flag("--session-file", `C:/fake-pi/sessions/session-${process.pid}.jsonl`);

if (heartbeat) {
  // Used by the orphan test: the file stops growing when this process dies.
  appendFileSync(heartbeat, `${process.pid}\n`);
  setInterval(() => appendFileSync(heartbeat, "beat\n"), 50);
}

function write(record) {
  process.stdout.write(`${JSON.stringify(record)}\n`);
}

function emit(type, extra = {}) {
  write({ type, ...extra });
}

function respond(id, type, data) {
  write({ id, type: "response", command: type, success: true, data });
}

function pause(ms = delayMs) {
  return ms > 0 ? new Promise((resolve) => setTimeout(resolve, ms)) : Promise.resolve();
}

let running = false;
let aborted = false;
const queue = [];

function assistantText(text) {
  return { role: "assistant", content: [{ type: "text", text }], stopReason: "stop" };
}

async function runTurn(text) {
  running = true;
  aborted = false;

  emit("agent_start");
  emit("turn_start");
  emit("message_start", { message: { role: "assistant", content: [], stopReason: "pending" } });

  if (scenario === "unknown") {
    process.stdout.write("this is not json at all\n");
    process.stdout.write("\n");
    write([1, 2, 3]);
    emit("from_the_future", { payload: { anything: true } });
    emit("extension_ui_request", {
      id: "ui-1",
      method: "confirm",
      title: "Allow bash?",
      message: "rm -rf build",
    });
    emit("extension_error", { error: "an extension blew up" });
    emit("compaction_start", { reason: "threshold" });
    emit("bash_execution_update", { id: "req-9", delta: "total 48\n" });
    emit("tool_execution_update", {
      toolCallId: "call_progress",
      toolName: "bash",
      partialResult: { content: [{ type: "text", text: "partial" }] },
    });
    await pause();
  }

  if (scenario === "crash") {
    emit("message_update", {
      assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "Working on it" },
    });
    await pause(30);
    process.exit(9);
  }

  if (scenario === "tools") {
    emit("tool_execution_start", {
      toolCallId: "call_1",
      toolName: "bash",
      args: { command: "ls -la" },
    });
    await pause();
    emit("tool_execution_end", {
      toolCallId: "call_1",
      toolName: "bash",
      result: { content: [{ type: "text", text: "total 0\nREADME.md" }] },
      isError: false,
      durationMs: 12,
    });
    await pause();
    emit("tool_execution_start", {
      toolCallId: "call_2",
      toolName: "read",
      args: { path: "C:/work/grove/README.md" },
    });
    await pause();
    emit("tool_execution_end", {
      toolCallId: "call_2",
      toolName: "read",
      result: { content: [{ type: "text", text: "error: no such file" }] },
      isError: true,
      durationMs: 3,
    });
    await pause();
  }

  emit("message_update", {
    assistantMessageEvent: { type: "thinking_start", contentIndex: 0 },
  });
  emit("message_update", {
    assistantMessageEvent: { type: "thinking_delta", contentIndex: 0, delta: "Answer briefly." },
  });
  emit("message_update", {
    assistantMessageEvent: { type: "thinking_end", contentIndex: 0 },
  });
  await pause();

  const answer = `Fake pi reply: ${text}`;
  for (const chunk of answer.split(" ")) {
    emit("message_update", {
      usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: {} },
      assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: `${chunk} ` },
    });
    await pause();
    if (aborted) return;
  }
  emit("message_update", { assistantMessageEvent: { type: "text_end", contentIndex: 0 } });
  emit("message_update", {
    usage: { input: 42, output: 11, cacheRead: 0, cacheWrite: 0, totalTokens: 53, cost: {} },
    assistantMessageEvent: { type: "done", reason: "stop" },
  });
  emit("message_end", { message: assistantText(answer.trim()) });
  emit("turn_end", { message: {}, toolResults: [] });
  // `agent_end` is *not* the end of work — the sidecar must wait for `agent_settled`.
  emit("agent_end", { messages: [], willRetry: false });
  await pause();
  emit("agent_settled", { aborted: false });
  running = false;

  if (queue.length > 0) await runTurn(queue.shift());
}

function onCommand(message) {
  const id = message.id;
  switch (message.type) {
    case "prompt": {
      respond(id, "prompt", { disposition: running ? "queued" : "started" });
      if (running) {
        queue.push(message.message);
        return;
      }
      void runTurn(message.message);
      return;
    }
    case "abort": {
      respond(id, "abort", {});
      if (running) {
        aborted = true;
        running = false;
        emit("agent_settled", { aborted: true });
      }
      return;
    }
    case "switch_session": {
      sessionFile = message.sessionPath;
      respond(id, "switch_session", {});
      return;
    }
    case "get_session_stats": {
      respond(id, "get_session_stats", {
        sessionFile,
        sessionId: "fake-pi-1",
        userMessages: 1,
        assistantMessages: 1,
        toolCalls: 0,
        toolResults: 0,
        totalMessages: 2,
        tokens: { input: 42, output: 11, cacheRead: 0, cacheWrite: 0, total: 53 },
        cost: 0,
        contextUsage: { tokens: 53, contextWindow: 200000, percent: 1 },
      });
      return;
    }
    default: {
      write({
        id,
        type: "response",
        command: message.type,
        success: false,
        error: `unknown command: ${message.type}`,
      });
    }
  }
}

// LF-only framing: split on "\n" and tolerate a preceding CR. Never `readline` (it also splits on
// U+2028/U+2029, which are legal inside JSON strings).
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
        onCommand(JSON.parse(line));
      } catch (error) {
        process.stderr.write(`fake_pi_rpc: bad command line: ${error.message}\n`);
      }
    }
    index = buffer.indexOf("\n");
  }
});
