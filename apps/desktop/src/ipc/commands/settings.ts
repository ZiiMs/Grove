import type {
  ProviderStatus,
  SecretStatusView,
  Settings,
  SettingsPatch,
  ThinkingLevel,
} from "@grove/contract";

/**
 * Settings, secrets and provider credentials. Secret commands expose status / set / delete only:
 * there is no command that returns a value, so a key can never be read back into the webview.
 */
export interface SettingsCommands {
  settings_get: { args: Record<string, never>; result: Settings };
  settings_set: { args: { patch: SettingsPatch }; result: Settings };
  secret_status: { args: { key: string }; result: SecretStatusView };
  secret_set: { args: { key: string; value: string }; result: SecretStatusView };
  secret_delete: { args: { key: string }; result: SecretStatusView };
  /** Every provider's readiness: configured, how it was found, and why it may be unusable. */
  provider_statuses: { args: Record<string, never>; result: ProviderStatus[] };
  /** Store a key. Returns the refreshed row; the value never comes back. */
  provider_key_set: { args: { provider: string; key: string }; result: ProviderStatus };
  /** Remove a stored key. The vendor's environment variable, if set, then applies again. */
  provider_key_delete: { args: { provider: string }; result: ProviderStatus };
  /** Ask the vendor which models the key can reach; this is the "test key" action. */
  provider_models: { args: { provider: string }; result: string[] };
  /** Provider id → model id → reasoning levels, from the cached models.dev catalogue. */
  provider_thinking: {
    args: Record<string, never>;
    result: Record<string, Record<string, ThinkingLevel[]>>;
  };
}
