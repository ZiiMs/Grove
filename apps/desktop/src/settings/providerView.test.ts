import { describe, expect, it } from "vitest";
import { FAKE_PROVIDER_ROSTER, fakeProviderStatus as provider } from "../test-utils/fakeBridge";
import {
  credentialOrigin,
  filterProviders,
  providerCounts,
  providerState,
  validateProviderKey,
} from "./providerView";

describe("provider state", () => {
  it("reads a configured credential as connected", () => {
    expect(providerState(provider(), false)).toEqual({ tone: "on", label: "Connected" });
  });

  it("marks a configured provider as rejected only after its key failed", () => {
    expect(providerState(provider(), true)).toEqual({
      tone: "reauth",
      label: "Key rejected",
    });
  });

  it("reads a missing credential as not connected, even if a rejection is stale", () => {
    expect(providerState(provider({ configured: false, source: null }), true)).toEqual({
      tone: "off",
      label: "Not connected",
    });
  });
});

describe("provider counts", () => {
  it("counts connected providers, their models, and what is left to add", () => {
    expect(providerCounts(FAKE_PROVIDER_ROSTER)).toEqual({
      connected: 2,
      available: 2,
      models: 5,
      total: 4,
    });
  });

  it("counts nothing when no credential is configured", () => {
    const rows = [provider({ configured: false, source: null })];

    expect(providerCounts(rows)).toEqual({ connected: 0, available: 1, models: 0, total: 1 });
  });
});

describe("credential origin", () => {
  it("names the keychain when Grove stored the key", () => {
    expect(credentialOrigin(provider())).toBe("in the OS keychain");
  });

  it("names the vendor variable when the key came from the environment", () => {
    expect(credentialOrigin(provider({ source: "environment" }))).toBe("from ANTHROPIC_API_KEY");
  });
});

describe("catalogue search", () => {
  const rows = FAKE_PROVIDER_ROSTER;

  it("returns the catalogue in order for an empty query", () => {
    expect(filterProviders(rows, "   ")).toEqual(rows);
  });

  it("matches a provider name case-insensitively", () => {
    expect(filterProviders(rows, "OPEN").map((row) => row.id)).toEqual(["openai", "openrouter"]);
  });

  it("matches a model id and the environment variable", () => {
    expect(filterProviders(rows, "claude-sonnet").map((row) => row.id)).toEqual(["anthropic", "openrouter"]);
    expect(filterProviders(rows, "OPENROUTER_API_KEY").map((row) => row.id)).toEqual(["openrouter"]);
  });

  it("is empty when nothing matches, so the dialog can say so", () => {
    expect(filterProviders(rows, "mistral")).toEqual([]);
  });
});

describe("key validation", () => {
  it("rejects an empty paste", () => {
    expect(validateProviderKey("   ")).toEqual({ ok: false, error: "Paste a key to continue." });
  });

  it("rejects a key with a line break or space inside", () => {
    expect(validateProviderKey("sk-ant-abc def")).toEqual({
      ok: false,
      error: "That key has a space or line break in it — paste it again.",
    });
  });

  it("rejects a truncated paste", () => {
    expect(validateProviderKey("short")).toEqual({
      ok: false,
      error: "That key looks too short — paste the whole thing.",
    });
  });

  it("trims the paste the way the core does", () => {
    expect(validateProviderKey("  sk-ant-api03-abcdef  \n")).toEqual({
      ok: true,
      key: "sk-ant-api03-abcdef",
    });
  });
});
