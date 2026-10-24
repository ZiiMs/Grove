import type {
  DoctorReport,
  SecretStatusView,
  Settings,
  SettingsPatch,
} from "@grove/contract";

/**
 * Settings, secrets and the doctor. Secret commands expose status / set / delete only: there is no
 * command that returns a value, so a secret can never be read back into the webview.
 */
export interface SettingsCommands {
  settings_get: { args: Record<string, never>; result: Settings };
  settings_set: { args: { patch: SettingsPatch }; result: Settings };
  secret_status: { args: { key: string }; result: SecretStatusView };
  secret_set: { args: { key: string; value: string }; result: SecretStatusView };
  secret_delete: { args: { key: string }; result: SecretStatusView };
  doctor_report: { args: Record<string, never>; result: DoctorReport };
}
