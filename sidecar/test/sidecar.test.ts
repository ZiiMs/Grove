import { PassThrough } from "node:stream";
import { describe, expect, it } from "vitest";
import { attachSidecar, type SidecarHostFactory, type SidecarNotifications } from "../src/sidecar";
import { createLineSplitter } from "../src/ndjson";
import {
  PROTOCOL_VERSION,
  RPC_INVALID_PARAMS,
  RPC_METHOD_NOT_FOUND,
  RPC_PARSE_ERROR,
  SidecarFailure,
  type SidecarHost,
} from "../src/rpc";

/** A host that records what the protocol layer asked it to do. */
function fakeHost(calls: string[]): SidecarHost {
  return {
    info: () => ({
      protocolVersion: PROTOCOL_VERSION,
      version: "0.1.0",
      harnesses: ["pi"],
      pi: { available: true, command: "pi" },
    }),
    start: async (params) => {
      calls.push(`start:${params.sessionId}`);
      return { resumeToken: "C:/sessions/one.jsonl" };
    },
    send: async (params) => {
      calls.push(`send:${params.sessionId}:${params.text}`);
    },
    permission: async (params) => {
      calls.push(`permission:${params.sessionId}:${params.decision}`);
    },
    cancel: async (params) => {
      calls.push(`cancel:${params.sessionId}`);
    },
    stop: async (params) => {
      calls.push(`stop:${params.sessionId}`);
    },
    shutdown: async () => {
      calls.push("shutdown");
    },
  };
}

/** Wire the protocol to in-memory streams and collect what the sidecar writes. */
function harness(factory?: (notify: SidecarNotifications) => SidecarHost) {
  const calls: string[] = [];
  const input = new PassThrough();
  const output = new PassThrough();
  const written: string[] = [];
  output.setEncoding("utf8");
  const splitter = createLineSplitter((line) => written.push(line));
  output.on("data", (chunk: string) => splitter.push(chunk));

  let notify: SidecarNotifications | undefined;
  const create: SidecarHostFactory = (notifications) => {
    notify = notifications;
    return factory ? factory(notifications) : fakeHost(calls);
  };
  attachSidecar({ input, output }, create);

  return {
    calls,
    written,
    notify(): SidecarNotifications {
      if (!notify) throw new Error("the host factory was never called");
      return notify;
    },
    send(line: string) {
      input.write(`${line}\n`);
    },
    async waitFor(predicate: () => boolean) {
      for (let attempt = 0; attempt < 200; attempt += 1) {
        if (predicate()) return;
        await new Promise((resolve) => setTimeout(resolve, 5));
      }
      throw new Error(`timed out; wrote: ${written.join("\n")}`);
    },
    async end() {
      input.end();
      await new Promise((resolve) => setTimeout(resolve, 30));
    },
  };
}

const request = (method: string, params?: unknown, id: string | number = 1) =>
  JSON.stringify({ jsonrpc: "2.0", id, method, params });

describe("sidecar process wiring", () => {
  it("answers initialize over the line protocol", async () => {
    const wire = harness();
    wire.send(request("initialize", { protocolVersion: 1 }, "init"));

    await wire.waitFor(() => wire.written.length === 1);
    const response = JSON.parse(wire.written[0]);
    expect(response.id).toBe("init");
    expect(response.result).toMatchObject({ protocolVersion: PROTOCOL_VERSION, harnesses: ["pi"] });
    await wire.end();
  });

  it("keeps a U+2028 inside a JSON string instead of splitting the record", async () => {
    const wire = harness();
    const text = "one\u2028two";
    wire.send(request("session.send", { sessionId: "s1", text }, "send"));

    // One request in, one response out: a `readline`-style split would have produced a parse error
    // plus a truncated request.
    await wire.waitFor(() => wire.written.length === 1);
    expect(JSON.parse(wire.written[0]).id).toBe("send");
    expect(wire.calls).toEqual([`send:s1:${text}`]);
    await wire.end();
  });

  it("answers session.start with the resumed token and forwards commands in order", async () => {
    const wire = harness();
    wire.send(request("session.start", { sessionId: "s1", cwd: "C:/work" }, 1));
    wire.send(request("session.send", { sessionId: "s1", text: "hello" }, 2));
    wire.send(request("session.cancel", { sessionId: "s1" }, 3));
    wire.send(request("session.stop", { sessionId: "s1" }, 4));

    await wire.waitFor(() => wire.written.length === 4);
    expect(JSON.parse(wire.written[0]).result).toEqual({ resumeToken: "C:/sessions/one.jsonl" });
    expect(wire.calls).toEqual(["start:s1", "send:s1:hello", "cancel:s1", "stop:s1"]);
    await wire.end();
  });

  it("writes session.event notifications with the session id and the normalized event", async () => {
    const wire = harness(() => ({
      ...fakeHost([]),
      start: async () => {
        setImmediate(() =>
          wire.notify().event("s1", {
            at: new Date().toISOString(),
            kind: "delta",
            text: "hi",
          }),
        );
        return {};
      },
    }));
    wire.send(request("session.start", { sessionId: "s1", cwd: "C:/work" }, 1));

    await wire.waitFor(() => wire.written.length === 2);
    const notification = JSON.parse(wire.written[1]);
    expect(notification).toMatchObject({
      jsonrpc: "2.0",
      method: "session.event",
      params: { sessionId: "s1", event: { kind: "delta", text: "hi" } },
    });
    expect(notification.id).toBeUndefined();
    await wire.end();
  });

  it("reports a parse error for junk and keeps answering", async () => {
    const wire = harness();
    wire.send("this is not json");
    wire.send(request("does.not.exist", {}, 2));

    await wire.waitFor(() => wire.written.length === 2);
    expect(JSON.parse(wire.written[0]).error.code).toBe(RPC_PARSE_ERROR);
    expect(JSON.parse(wire.written[1]).error.code).toBe(RPC_METHOD_NOT_FOUND);
    await wire.end();
  });

  it("rejects a request with missing params instead of starting a phantom session", async () => {
    const wire = harness();
    wire.send(request("session.start", { cwd: "C:/work" }, 1));

    await wire.waitFor(() => wire.written.length === 1);
    expect(JSON.parse(wire.written[0]).error.code).toBe(RPC_INVALID_PARAMS);
    expect(wire.calls).toEqual([]);
    await wire.end();
  });

  it("maps a host failure onto its JSON-RPC code", async () => {
    const wire = harness(() => ({
      ...fakeHost([]),
      start: async () => {
        throw new SidecarFailure(-32000, "pi is not installed", { reason: "pi is not installed" });
      },
    }));
    wire.send(request("session.start", { sessionId: "s1", cwd: "C:/work" }, 7));

    await wire.waitFor(() => wire.written.length === 1);
    const response = JSON.parse(wire.written[0]);
    expect(response.id).toBe(7);
    expect(response.error).toMatchObject({
      code: -32000,
      message: "pi is not installed",
      data: { reason: "pi is not installed" },
    });
    await wire.end();
  });

  it("shuts the host down when the parent closes stdin", async () => {
    const wire = harness();
    await wire.end();
    expect(wire.calls).toEqual(["shutdown"]);
  });
});
