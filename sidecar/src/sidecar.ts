/**
 * The transport: newline-delimited JSON-RPC on stdio, wired to a [`SidecarHost`].
 *
 * Nothing but protocol may go to stdout — diagnostics go to stderr — because the parent parses
 * stdout. Requests are handled strictly in order, so a `session.send` can never overtake the
 * `session.start` it belongs to, and notifications (`session.event`) are interleaved as the host
 * emits them.
 */

import type { Readable, Writable } from "node:stream";
import type { HarnessEvent } from "./events.js";
import { createLineSplitter } from "./ndjson.js";
import {
  handleRequest,
  rpcError,
  RPC_PARSE_ERROR,
  type JsonRpcNotification,
  type JsonRpcResponse,
  type SidecarHost,
} from "./rpc.js";

export interface SidecarStreams {
  input: Readable;
  output: Writable;
}

/** How the host pushes events to the parent. `session.event` is the only notification v1 defines. */
export interface SidecarNotifications {
  event(sessionId: string, event: HarnessEvent): void;
}

/** Builds the host and hands it the notifier it should publish events through. */
export type SidecarHostFactory = (notify: SidecarNotifications) => SidecarHost;

export function attachSidecar(streams: SidecarStreams, createHost: SidecarHostFactory): SidecarHost {
  const { input, output } = streams;

  const write = (message: JsonRpcResponse | JsonRpcNotification): void => {
    output.write(`${JSON.stringify(message)}\n`);
  };
  const notify: SidecarNotifications = {
    event: (sessionId, event) =>
      write({ jsonrpc: "2.0", method: "session.event", params: { sessionId, event } }),
  };

  const host = createHost(notify);
  let chain: Promise<void> = Promise.resolve();

  const handleLine = async (line: string): Promise<void> => {
    let decoded: unknown;
    try {
      decoded = JSON.parse(line);
    } catch (error) {
      write(rpcError(null, RPC_PARSE_ERROR, `invalid JSON: ${(error as Error).message}`));
      return;
    }
    // A JSON-RPC notification carries no id; v1 accepts none from the parent and answers nothing.
    if (typeof decoded === "object" && decoded !== null && !("id" in decoded)) return;
    const response = await handleRequest(decoded, host);
    if (response) write(response);
  };

  const splitter = createLineSplitter((line) => {
    chain = chain.then(() => handleLine(line));
  });

  input.setEncoding("utf8");
  input.on("data", (chunk: string) => splitter.push(chunk));
  input.on("end", () => {
    splitter.end();
    chain = chain.then(() => host.shutdown());
  });
  input.on("error", (error: Error) => {
    process.stderr.write(`grove-sidecar: stdin failed: ${error.message}\n`);
    void host.shutdown();
  });

  return host;
}
