#!/usr/bin/env node
// A scripted stand-in for `claude --print --output-format stream-json --verbose`.
//
// It never touches the network and never calls a model: it replays the newline-delimited
// `stream-json` line shapes documented for the Claude Code CLI, so the adapter tests run offline and
// without a real `claude` install. Every mode is one fixture the adapter must survive:
//
//   happy       text answer, usage, success
//   tools       tool_use → tool_result (one ok, one error), then a final answer
//   permission  a permission control request the one-shot protocol cannot answer
//   error       a failing `result` line (error_max_turns)
//   crash       output, then the process dies mid-stream without a `result`
//   unknown     unknown and malformed lines interleaved with a valid answer
//   slow        text deltas with a delay, so a cancel has time to land mid-stream
//
// The prompt arrives on stdin, exactly like the one-shot CLI. `--resume` continues a conversation,
// so a resumed run echoes the id it was given instead of minting a new one.

const argv = process.argv.slice(2);

function flag(name, fallback = null) {
  const index = argv.indexOf(name);
  return index >= 0 && index + 1 < argv.length ? argv[index + 1] : fallback;
}

const mode = flag("--mode", "happy");
const delayMs = Number(flag("--delay-ms", "0"));
const sessionId = flag("--resume") ?? `fake-session-${process.pid}-${Date.now()}`;
const model = flag("--model", "claude-fake-1");

// The real CLI rejects `--output-format stream-json` without `--print` and `--verbose`. Failing here
// proves the adapter passes the documented flags rather than only looking like it does.
const missing = [];
if (!argv.includes("--print")) missing.push("--print");
if (flag("--output-format") !== "stream-json") missing.push("--output-format stream-json");
if (!argv.includes("--verbose")) missing.push("--verbose");
if (missing.length > 0) {
  process.stderr.write(`claude_fake: missing required flag(s): ${missing.join(", ")}\n`);
  process.exit(2);
}

function emit(line) {
  process.stdout.write(`${JSON.stringify(line)}\n`);
}

async function pause(ms = delayMs) {
  if (ms > 0) await new Promise((resolve) => setTimeout(resolve, ms));
}

async function readPrompt() {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf8");
}

function firstLine(text) {
  return text.split(/\r?\n/).find((line) => line.trim().length > 0)?.trim() ?? "";
}

function init(capabilities = ["interrupt_receipt_v1"]) {
  emit({
    type: "system",
    subtype: "init",
    session_id: sessionId,
    cwd: process.cwd(),
    model,
    tools: ["Read", "Edit", "Bash"],
    mcp_servers: [],
    permissionMode: "default",
    capabilities,
    uuid: "init-1",
  });
}

function result(overrides = {}) {
  emit({
    type: "result",
    subtype: "success",
    is_error: false,
    session_id: sessionId,
    result: "",
    num_turns: 1,
    duration_ms: 12,
    duration_api_ms: 9,
    total_cost_usd: 0,
    usage: { input_tokens: 12, output_tokens: 7 },
    modelUsage: {},
    permission_denials: [],
    ...overrides,
  });
}

async function main() {
  const prompt = await readPrompt();
  const echo = firstLine(prompt);

  switch (mode) {
    case "happy": {
      init();
      await pause();
      emit({
        type: "assistant",
        session_id: sessionId,
        parent_tool_use_id: null,
        uuid: "msg-1",
        message: {
          id: "msg_1",
          role: "assistant",
          content: [
            { type: "thinking", thinking: "The prompt is short, so answer it directly." },
            { type: "text", text: `Fake reply: ${echo}` },
          ],
        },
      });
      await pause();
      result({ result: `Fake reply: ${echo}` });
      return;
    }

    case "tools": {
      init();
      await pause();
      emit({
        type: "assistant",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          id: "msg_1",
          role: "assistant",
          content: [
            { type: "text", text: "Reading the README." },
            { type: "tool_use", id: "toolu_1", name: "Read", input: { file_path: "C:/work/grove/README.md" } },
          ],
        },
      });
      await pause();
      emit({
        type: "user",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          role: "user",
          content: [
            {
              type: "tool_result",
              tool_use_id: "toolu_1",
              content: [{ type: "text", text: "# Grove\n\nOne desktop home for every AI coding harness." }],
              is_error: false,
            },
          ],
        },
      });
      await pause();
      emit({
        type: "assistant",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          id: "msg_2",
          role: "assistant",
          content: [
            { type: "tool_use", id: "toolu_2", name: "Bash", input: { command: "cargo test -p grove-core" } },
          ],
        },
      });
      await pause();
      emit({
        type: "user",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          role: "user",
          content: [
            {
              type: "tool_result",
              tool_use_id: "toolu_2",
              content: [{ type: "text", text: "error: one test failed" }],
              is_error: true,
            },
          ],
        },
      });
      await pause();
      emit({
        type: "assistant",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          id: "msg_3",
          role: "assistant",
          content: [{ type: "text", text: "The suite has a failing test." }],
        },
      });
      await pause();
      result({ result: "The suite has a failing test.", usage: { input_tokens: 40, output_tokens: 15 } });
      return;
    }

    case "permission": {
      init([]);
      await pause();
      // The undocumented control channel: a one-shot `-p` run cannot answer this, so the adapter must
      // log it instead of pretending permissions are interactive.
      emit({
        type: "control_request",
        session_id: sessionId,
        request: {
          subtype: "permission",
          request_id: "perm_1",
          tool_name: "Bash",
          tool_input: { command: "rm -rf build" },
        },
      });
      await pause();
      emit({ type: "system", subtype: "permission_denied", session_id: sessionId });
      await pause();
      result({ result: "permission denied", usage: { input_tokens: 8, output_tokens: 3 } });
      return;
    }

    case "error": {
      init();
      await pause();
      emit({
        type: "assistant",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          id: "msg_1",
          role: "assistant",
          content: [{ type: "text", text: "Starting the task." }],
        },
      });
      await pause();
      result({
        subtype: "error_max_turns",
        is_error: true,
        result: "Reached the maximum number of turns.",
        usage: { input_tokens: 9, output_tokens: 2 },
      });
      process.exitCode = 1;
      return;
    }

    case "crash": {
      init();
      await pause();
      emit({
        type: "assistant",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          id: "msg_1",
          role: "assistant",
          content: [{ type: "text", text: "Working on it..." }],
        },
      });
      // Give the pipe a moment to flush, then die without a `result` line.
      await pause(30);
      process.exit(7);
    }

    case "unknown": {
      init();
      await pause();
      process.stdout.write("this is not json at all\n");
      process.stdout.write("\n");
      process.stdout.write("   \t \n");
      emit({ type: "from_the_future", session_id: sessionId, payload: { anything: true } });
      emit([1, 2, 3]);
      emit({ type: "assistant", session_id: sessionId });
      await pause();
      emit({
        type: "stream_event",
        session_id: sessionId,
        event: { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: "Hel" } },
      });
      emit({
        type: "stream_event",
        session_id: sessionId,
        event: { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: "lo" } },
      });
      emit({ type: "stream_event", session_id: sessionId, event: { type: "content_block_stop", index: 0 } });
      await pause();
      emit({
        type: "assistant",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          id: "msg_1",
          role: "assistant",
          content: [{ type: "text", text: "Hello" }],
        },
      });
      emit({ type: "result_without_a_body", session_id: sessionId });
      await pause();
      result({ result: "Hello", usage: { input_tokens: 3, output_tokens: 2 } });
      return;
    }

    case "slow": {
      init();
      await pause();
      for (let index = 1; index <= 30; index += 1) {
        emit({
          type: "stream_event",
          session_id: sessionId,
          event: {
            type: "content_block_delta",
            index: 0,
            delta: { type: "text_delta", text: `chunk ${index} ` },
          },
        });
        await pause();
      }
      emit({
        type: "assistant",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          id: "msg_1",
          role: "assistant",
          content: [{ type: "text", text: "A slow answer finished." }],
        },
      });
      result({ result: "A slow answer finished." });
      return;
    }

    default: {
      process.stderr.write(`claude_fake: unknown mode: ${mode}\n`);
      process.exit(2);
    }
  }
}

main().catch((error) => {
  process.stderr.write(`claude_fake: ${error?.stack ?? error}\n`);
  process.exitCode = 1;
});
