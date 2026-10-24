import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  appInfoSchema,
  eventEnvelopeSchema,
  ipcErrorSchema,
  parseEventEnvelope,
  specChangeSchema,
  specDocSchema,
  specGraphSchema,
  specNodeSchema,
  specValidationSchema,
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

describe("spec contract fixtures", () => {
  it("spec-node.json matches the zod schema", () => {
    const parsed = specNodeSchema.parse(fixture("spec-node.json"));
    expect(parsed.id).toBe("grove-architecture");
    expect(parsed.type).toBe("architecture-design");
    expect(parsed.dependsOn).toEqual(["grove-goal-and-requirements"]);
  });

  it("spec-validation.json matches the zod schema", () => {
    const parsed = specValidationSchema.parse(fixture("spec-validation.json"));
    expect(parsed.danglingLinks[0]?.target).toBe("grove-missing-spec");
    expect(parsed.duplicateIds[0]?.paths).toEqual(["specs/one.md", "specs/two.md"]);
    expect(parsed.parentCycles[0]?.ids).toEqual(["cycle-a", "cycle-b"]);
  });

  it("spec-graph.json matches the zod schema", () => {
    const parsed = specGraphSchema.parse(fixture("spec-graph.json"));
    expect(parsed.root).toBe("C:/demo/grove");
    expect(parsed.nodes).toHaveLength(1);
    expect(parsed.validation.parentCycles).toEqual([]);
  });

  it("spec-doc.json matches the zod schema", () => {
    const parsed = specDocSchema.parse(fixture("spec-doc.json"));
    expect(parsed.node.id).toBe("grove-architecture");
    expect(parsed.body).toContain("# Grove");
    expect(parsed.hash).toHaveLength(64);
  });

  it("spec-change.json matches the zod schema", () => {
    const parsed = specChangeSchema.parse(fixture("spec-change.json"));
    expect(parsed.paths).toEqual(["architecture.md", "goal-and-requirements.md"]);
  });
});
