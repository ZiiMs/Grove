#!/usr/bin/env node
// Fake MCP stdio server for Grove's probe tests.
//
//   node fake_stdio_server.mjs [--mode=ok|sleep|crash|error] [--touch=<path>] [--heartbeat=<path>]
//
//   ok      speaks newline-delimited JSON-RPC: answers `initialize` and `tools/list`, ignores
//           notifications, and exits when stdin closes.
//   sleep   never answers. Rewrites `--heartbeat` every 100 ms so a test can prove the probe killed
//           it instead of leaving an orphan behind.
//   crash   writes stderr and exits non-zero: the "server refuses to start" case.
//   error   answers `initialize` but replies to `tools/list` with a JSON-RPC error object.
//
// `--touch=<path>` is written on startup in every mode, so a test can assert whether anything was
// spawned at all (discovery must never spawn).

import { writeFileSync } from "node:fs";
import { createInterface } from "node:readline";

const PROTOCOL_VERSION = "2024-11-05";

function option(name, fallback) {
  const prefix = `--${name}=`;
  const hit = process.argv.slice(2).find((arg) => arg.startsWith(prefix));
  return hit ? hit.slice(prefix.length) : fallback;
}

function touch(path) {
  if (!path) return;
  try {
    writeFileSync(path, String(Date.now()));
  } catch {
    // A marker the test cannot see is a test problem, not a server problem.
  }
}

const mode = option("mode", "ok");
const touchPath = option("touch", "");
const heartbeatPath = option("heartbeat", "");

touch(touchPath);

if (mode === "crash") {
  process.stderr.write("fake mcp server: refusing to start\n");
  process.exit(3);
}

if (mode === "sleep") {
  process.stderr.write("fake mcp server: sleeping\n");
  if (heartbeatPath) {
    const beat = () => touch(heartbeatPath);
    beat();
    setInterval(beat, 100);
  }
  // Drain stdin and keep the process alive indefinitely.
  createInterface({ input: process.stdin }).on("line", () => {});
  setInterval(() => {}, 60_000);
} else {
  const tools = [
    { name: "read_file", description: "Read a file from the project" },
    { name: "list_directory" },
  ];
  const send = (message) => process.stdout.write(`${JSON.stringify(message)}\n`);

  const handle = (request) => {
    if (request.method === "initialize") {
      send({
        jsonrpc: "2.0",
        id: request.id,
        result: {
          protocolVersion: PROTOCOL_VERSION,
          capabilities: { tools: {} },
          serverInfo: { name: "grove-fake", version: "1.0.0" },
        },
      });
    } else if (request.method === "tools/list") {
      if (mode === "error") {
        send({
          jsonrpc: "2.0",
          id: request.id,
          error: { code: -32603, message: "tools are unavailable in this mode" },
        });
        return;
      }
      send({ jsonrpc: "2.0", id: request.id, result: { tools } });
    }
    // Notifications and unknown methods are ignored, as a forward-compatible server does.
  };

  const input = createInterface({ input: process.stdin });
  input.on("line", (line) => {
    let request;
    try {
      request = JSON.parse(line);
    } catch {
      return;
    }
    handle(request);
  });
  input.on("close", () => process.exit(0));
}
