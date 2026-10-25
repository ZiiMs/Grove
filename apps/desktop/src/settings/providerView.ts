import type { ProviderStatus } from "@grove/contract";

/** The three states a provider card draws: ready, rejected, or nothing configured. */
export type ProviderTone = "on" | "reauth" | "off";

export interface ProviderState {
  tone: ProviderTone;
  label: string;
}

/**
 * What one provider row says on its card. `rejected` is a message the current pane got back from a
 * vendor that refused the credential; it clears when a new key is saved or the pane remounts.
 */
export function providerState(row: ProviderStatus, rejected: boolean): ProviderState {
  if (!row.configured) return { tone: "off", label: "Not connected" };
  if (rejected) return { tone: "reauth", label: "Key rejected" };
  return { tone: "on", label: "Connected" };
}

export interface ProviderCounts {
  connected: number;
  available: number;
  /** Models reachable through the credentials on this machine, not the size of the catalogue. */
  models: number;
  total: number;
}

export function providerCounts(rows: ProviderStatus[]): ProviderCounts {
  const connected = rows.filter((row) => row.configured);
  return {
    connected: connected.length,
    available: rows.length - connected.length,
    models: connected.reduce((sum, row) => sum + row.models.length, 0),
    total: rows.length,
  };
}

/** Where the credential came from, in the words the settings pane uses. */
export function credentialOrigin(row: ProviderStatus): string {
  if (row.source === "environment") return `from ${row.envVar}`;
  return "in the OS keychain";
}

/**
 * The catalogue rows matching a query. A user looking for a key knows a model name or the variable
 * they already export, so all of those count, not just the vendor name.
 */
export function filterProviders(rows: ProviderStatus[], query: string): ProviderStatus[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return rows;
  return rows.filter((row) =>
    [row.name, row.id, row.short, row.envVar, row.defaultModel, row.keyUrl, ...row.models].some(
      (value) => value.toLowerCase().includes(needle),
    ),
  );
}

export type KeyCheck = { ok: true; key: string } | { ok: false; error: string };

/**
 * What the dialog can know before it asks the core to store a key. The core still rejects an empty or
 * control-laden value; this only catches a paste that is visibly broken, so the user does not wait
 * for a keychain round trip to learn it.
 */
export function validateProviderKey(raw: string): KeyCheck {
  const key = raw.trim();
  if (!key) return { ok: false, error: "Paste a key to continue." };
  if (/\s/.test(key)) {
    return { ok: false, error: "That key has a space or line break in it — paste it again." };
  }
  if (key.length < 8) {
    return { ok: false, error: "That key looks too short — paste the whole thing." };
  }
  return { ok: true, key };
}
