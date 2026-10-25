import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";
import { createPiHost } from "../src/pi_host";
import { RPC_NOT_FOUND, RPC_UNAVAILABLE, RPC_UNSUPPORTED, type SidecarHost } from "../src/rpc";
import type { HarnessEvent } from "../src/events";

const FAKE_PI = fileURLToPath(new URL("./fixtures/fake_pi_rpc.mjs", import.meta.url));

/** A pi config that points the sidecar at the scripted fake instead of a real install. */
function piConfig(scenario: string, extra: string[] = []) {
  return {
    command: process.execPath,
    args: [FAKE_PI, "--mode", "rpc", "--scenario", scenario, ...extra],
  };
}

interface Rig {
  host: SidecarHost;
  events: HarnessEvent[];
  sessionEvents(sessionId?: string): HarnessEvent[];
  waitFor(predicate: () => boolean, label?: string): Promise<void>;
  send(sessionId: string, text: string): Promise<void>;
  kinds(): string[];
}

const rigs: Rig[] = [];

function rig(): Rig {
  const events: HarnessEvent[] = [];
  const bySession = new Map<string, HarnessEvent[]>();
  const host = createPiHost({
    log: () => {},
    notify: {
      event: (sessionId, event) => {
        events.push(event);
        const bucket = bySession.get(sessionId) ?? [];
        bucket.push(event);
        bySession.set(sessionId, bucket);
      },
    },
  });
  const instance: Rig = {
    host,
    events,
    sessionEvents: (sessionId) => (sessionId ? bySession.get(sessionId) ?? [] : events),
    async waitFor(predicate, label = "events") {
      for (let attempt = 0; attempt < 400; attempt += 1) {
        if (predicate()) return;
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
      throw new Error(
        `timed out waiting for ${label}: ${JSON.stringify(events.map((event) => event.kind))}`,
      );
    },
    send: (sessionId, text) => host.send({ sessionId, text }),
    kinds: () => events.map((event) => event.kind),
  };
  rigs.push(instance);
  return instance;
}

afterEach(async () => {
  while (rigs.length > 0) await rigs.pop()?.host.shutdown();
});

const isIdle = (event: HarnessEvent) => event.kind === "status" && event.state === "idle";

describe("pi host", () => {
  it("reports unavailable with a reason when the pi binary is absent", async () => {
    const { host } = rig();
    const unavailable = "grove-definitely-not-a-real-pi-binary";
    await expect(
      host.start({
        sessionId: "s1",
        cwd: tempdir(),
        config: { pi: { command: unavailable } },
      }),
    ).rejects.toMatchObject({
      code: RPC_UNAVAILABLE,
      message: expect.stringContaining(unavailable),
    });

    // `initialize` describes the default resolution, not a per-session override.
    const created = createPiHost({
      log: () => {},
      notify: { event: () => {} },
      config: { command: unavailable },
    });
    expect(created.info().pi.available).toBe(false);
    expect(created.info().pi.reason).toContain(unavailable);
    await created.shutdown();
  });

  it("maps a happy turn onto session_started, ready, deltas, a message, usage and idle", async () => {
    const test = rig();
    const launch = piConfig("happy");
    const result = await test.host.start({
      sessionId: "s1",
      cwd: tempdir(),
      model: "pi-fake-1",
      config: { pi: launch },
    });
    expect(result.resumeToken?.endsWith(".jsonl")).toBe(true);

    const startup = test.sessionEvents("s1");
    expect(startup[0]).toMatchObject({
      kind: "session_started",
      harness: "pi",
      model: "pi-fake-1",
      resumed: false,
    });
    expect(startup[1]).toMatchObject({ kind: "status", state: "starting" });
    expect(startup[2]).toMatchObject({ kind: "status", state: "ready" });

    await test.send("s1", "hello pi");
    await test.waitFor(() => test.sessionEvents("s1").some(isIdle), "idle");

    const turn = test.sessionEvents("s1").slice(3);
    expect(turn[0]).toMatchObject({ kind: "status", state: "busy" });
    expect(turn.some((event) => event.kind === "delta")).toBe(true);
    const message = turn.find(
      (event) => event.kind === "message" && event.blocks.some((block) => block.type === "text"),
    );
    expect(message).toMatchObject({ role: "assistant" });
    expect(message?.kind === "message" ? message.blocks.at(-1) : undefined).toEqual({
      type: "text",
      text: "Fake pi reply: hello pi",
    });
    expect(turn.some((event) => event.kind === "usage" && event.input_tokens === 42)).toBe(true);
    expect(turn[turn.length - 1]).toMatchObject({ kind: "status", state: "idle" });
    // `agent_end` is not the settled signal: the turn ends on `agent_settled`, once.
    expect(turn.filter(isIdle)).toHaveLength(1);
    // One assistant message, thinking first, and only after the streamed deltas.
    expect(turn.filter((event) => event.kind === "message").map((event) => event.blocks.map((block) => block.type))).toEqual([
      ["thinking", "text"],
    ]);
  });

  it("prepends the rendered context documents to the first prompt", async () => {
    const test = rig();
    await test.host.start({
      sessionId: "s1",
      cwd: tempdir(),
      prompt: "do the thing",
      context: [{ id: "grove-architecture", title: "Grove — architecture", body: "# The contract" }],
      config: { pi: piConfig("happy") },
    });
    await test.waitFor(() => test.sessionEvents("s1").some(isIdle), "idle");
    const text = test
      .sessionEvents("s1")
      .filter((event) => event.kind === "message")
      .flatMap((event) => (event.kind === "message" ? event.blocks : []))
      .map((block) => block.text)
      .join("\n");
    expect(text).toContain("# Context");
    expect(text).toContain("grove-architecture");
    expect(text).toContain("# The contract");
    expect(text).toContain("---\n\ndo the thing");
  });

  it("maps tool_execution_start/end onto a tool call and its result", async () => {
    const test = rig();
    await test.host.start({
      sessionId: "s1",
      cwd: tempdir(),
      config: { pi: piConfig("tools") },
    });
    await test.send("s1", "inspect the repo");
    await test.waitFor(() => test.sessionEvents("s1").some(isIdle), "idle");

    const calls = test.sessionEvents("s1").filter((event) => event.kind === "tool_call");
    expect(calls).toHaveLength(2);
    expect(calls[0]).toMatchObject({ call_id: "call_1", name: "bash", title: "ls -la" });
    expect(calls[1]).toMatchObject({ call_id: "call_2", name: "read", title: "C:/work/grove/README.md" });

    const results = test.sessionEvents("s1").filter((event) => event.kind === "tool_result");
    expect(results[0]).toMatchObject({
      call_id: "call_1",
      status: "ok",
      summary: "total 0\nREADME.md",
    });
    expect(results[1]).toMatchObject({ call_id: "call_2", status: "error" });

    // A call always precedes its result.
    const kinds = test.kinds();
    expect(kinds.indexOf("tool_call")).toBeLessThan(kinds.indexOf("tool_result"));
  });

  it("maps thinking deltas onto a thinking block", async () => {
    const test = rig();
    await test.host.start({
      sessionId: "s1",
      cwd: tempdir(),
      config: { pi: piConfig("happy") },
    });
    await test.send("s1", "think");
    await test.waitFor(() => test.sessionEvents("s1").some(isIdle), "idle");

    expect(
      test
        .sessionEvents("s1")
        .some(
          (event) =>
            event.kind === "message" &&
            event.blocks[0]?.type === "thinking" &&
            event.blocks[0].text === "Answer briefly.",
        ),
    ).toBe(true);
  });

  it("cancels by aborting pi, returns to idle, and stays usable", async () => {
    const test = rig();
    await test.host.start({
      sessionId: "s1",
      cwd: tempdir(),
      config: { pi: piConfig("slow", ["--delay-ms", "40"]) },
    });
    await test.send("s1", "a long slow answer");
    await test.waitFor(
      () => test.sessionEvents("s1").some((event) => event.kind === "delta"),
      "a delta",
    );

    await test.host.cancel({ sessionId: "s1" });
    await test.waitFor(() => test.sessionEvents("s1").some(isIdle), "idle");
    const afterCancel = test.sessionEvents("s1");
    const idleAt = afterCancel.findIndex(isIdle);
    expect(
      afterCancel
        .slice(idleAt)
        .some((event) => event.kind === "message" || event.kind === "error"),
    ).toBe(false);
    expect(
      afterCancel.some(
        (event) => event.kind === "message" && event.blocks.some((block) => block.type === "text"),
      ),
    ).toBe(false);

    await test.send("s1", "after the cancel");
    await test.waitFor(
      () => test.sessionEvents("s1").filter(isIdle).length >= 2,
      "a second idle",
    );
    expect(test.sessionEvents("s1").some((event) => event.kind === "message")).toBe(true);
  });

  it("resumes with the session file path from get_session_stats", async () => {
    const first = rig();
    const started = await first.host.start({
      sessionId: "s1",
      cwd: tempdir(),
      config: { pi: piConfig("happy") },
    });
    const token = started.resumeToken!;
    expect(token).toContain("session-");
    await first.host.stop({ sessionId: "s1" });

    const second = rig();
    const resumed = await second.host.start({
      sessionId: "s2",
      cwd: tempdir(),
      resumeToken: token,
      config: { pi: piConfig("happy") },
    });
    expect(resumed.resumeToken).toBe(token);
    expect(second.sessionEvents("s2")[0]).toMatchObject({ kind: "session_started", resumed: true });
  });

  it("reports a dead pi process as a retryable error and exited", async () => {
    const test = rig();
    await test.host.start({
      sessionId: "s1",
      cwd: tempdir(),
      config: { pi: piConfig("crash") },
    });
    await test.send("s1", "go");
    await test.waitFor(
      () => test.sessionEvents("s1").some((event) => event.kind === "status" && event.state === "exited"),
      "exited",
    );
    const tail = test.sessionEvents("s1").slice(-2);
    expect(tail[0]).toMatchObject({ kind: "error", retryable: true });
    expect(tail[1]).toMatchObject({ kind: "status", state: "exited" });
  });

  it("drops unknown records, extension UI traffic and junk lines", async () => {
    const test = rig();
    await test.host.start({
      sessionId: "s1",
      cwd: tempdir(),
      config: { pi: piConfig("unknown") },
    });
    await test.send("s1", "still works");
    await test.waitFor(() => test.sessionEvents("s1").some(isIdle), "idle");

    expect(test.sessionEvents("s1").some((event) => event.kind === "error")).toBe(false);
    expect(test.sessionEvents("s1").some((event) => event.kind === "delta")).toBe(true);
    expect(
      test.sessionEvents("s1").some(
        (event) => event.kind === "message" && event.blocks.at(-1)?.text === "Fake pi reply: still works",
      ),
    ).toBe(true);
  });

  it("kills the pi child on stop and on shutdown", async () => {
    const heartbeat = join(mkdtempSync(join(tmpdir(), "grove-pi-hb-")), "beat.txt");
    const test = rig();
    await test.host.start({
      sessionId: "s1",
      cwd: tempdir(),
      config: { pi: piConfig("happy", ["--heartbeat", heartbeat]) },
    });
    await waitForFile(heartbeat);
    const before = readFileSync(heartbeat, "utf8").length;
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(readFileSync(heartbeat, "utf8").length).toBeGreaterThan(before);

    await test.host.stop({ sessionId: "s1" });
    const afterStop = readFileSync(heartbeat, "utf8").length;
    await new Promise((resolve) => setTimeout(resolve, 250));
    expect(
      readFileSync(heartbeat, "utf8").length,
      "a stopped session leaves no pi process behind",
    ).toBe(afterStop);

    // Same guarantee for the whole sidecar going away.
    const secondHeartbeat = join(mkdtempSync(join(tmpdir(), "grove-pi-hb-")), "beat.txt");
    const other = rig();
    await other.host.start({
      sessionId: "s2",
      cwd: tempdir(),
      config: { pi: piConfig("happy", ["--heartbeat", secondHeartbeat]) },
    });
    await waitForFile(secondHeartbeat);
    await other.host.shutdown();
    const afterShutdown = readFileSync(secondHeartbeat, "utf8").length;
    await new Promise((resolve) => setTimeout(resolve, 250));
    expect(readFileSync(secondHeartbeat, "utf8").length).toBe(afterShutdown);
  });

  it("refuses session.send for an unknown session", async () => {
    const { host } = rig();
    await expect(host.send({ sessionId: "nope", text: "hi" })).rejects.toMatchObject({
      code: RPC_NOT_FOUND,
    });
    await expect(host.cancel({ sessionId: "nope" })).resolves.toBeUndefined();
  });

  it("reports interactive permissions as unsupported", async () => {
    const { host } = rig();
    await expect(
      host.permission({ sessionId: "s1", requestId: "perm-1", decision: "approve" }),
    ).rejects.toMatchObject({ code: RPC_UNSUPPORTED });
  });
});

function tempdir(): string {
  return mkdtempSync(join(tmpdir(), "grove-pi-"));
}

async function waitForFile(path: string): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    if (existsSync(path)) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error(`the fake pi never wrote ${path}`);
}
