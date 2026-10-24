import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  appInfoSchema,
  eventEnvelopeSchema,
  ipcErrorSchema,
  parseEventEnvelope,
} from "../src/index";

function fixture(name: string): unknown {
  const path = fileURLToPath(new URL(`../../../contract/fixtures/${name}`, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8"));
}

describe("contract fixtures", () => {
  it("app-info.json matches the zod schema", () => {
    const parsed = appInfoSchema.parse(fixture("app-info.json"));
    expect(parsed.name).toBe("Grove");
  });

  it("event-envelope.json matches the zod schema", () => {
    const parsed = eventEnvelopeSchema.parse(fixture("event-envelope.json"));
    expect(parsed.payload.kind).toBe("status");
  });

  it("ipc-error.json matches the zod schema", () => {
    const parsed = ipcErrorSchema.parse(fixture("ipc-error.json"));
    expect(parsed.code).toBe("conflict");
    expect(parsed.detail).toBe("expected version 3");
  });
});

describe("parseEventEnvelope", () => {
  it("accepts a known kind", () => {
    expect(parseEventEnvelope(fixture("event-envelope.json"))).not.toBeNull();
  });

  it("ignores an unknown kind instead of throwing", () => {
    expect(
      parseEventEnvelope({ sessionId: "s", payload: { kind: "from-the-future" } }),
    ).toBeNull();
  });

  it("ignores a malformed envelope", () => {
    expect(parseEventEnvelope({ payload: { kind: 42 } })).toBeNull();
  });
});
