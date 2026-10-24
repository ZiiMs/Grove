#!/usr/bin/env node
/**
 * The sidecar process. Reads newline-delimited JSON-RPC from stdin, writes responses to stdout.
 *
 * Nothing but protocol may go to stdout — diagnostics go to stderr, because the parent parses stdout.
 */
import { createInterface } from "node:readline";
import { handleRequest, rpcError, RPC_PARSE_ERROR, type JsonRpcResponse } from "./rpc.js";

function write(message: JsonRpcResponse): void {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });

lines.on("line", (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;

  let decoded: unknown;
  try {
    decoded = JSON.parse(trimmed);
  } catch (error) {
    write(rpcError(null, RPC_PARSE_ERROR, `invalid JSON: ${(error as Error).message}`));
    return;
  }

  // Notifications (no id) are accepted silently; the shell currently sends none, but the protocol
  // reserves them for cancellation.
  if (typeof decoded === "object" && decoded !== null && !("id" in decoded)) return;

  write(handleRequest(decoded));
});

lines.on("close", () => {
  process.exit(0);
});

process.on("uncaughtException", (error) => {
  console.error("sidecar uncaught exception:", error);
  process.exit(1);
});
