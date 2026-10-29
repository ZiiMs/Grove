import type { GroveBridge } from "../ipc/bridge";
import { errorText } from "../ipc/errors";

/**
 * Show a path in the platform file manager. This is the one wrapper around `app_reveal_path`: the
 * command never throws at a caller, and a failure is reported the same way a panel reports any other.
 */
export async function revealPath(
  bridge: GroveBridge,
  path: string,
  notify: (message: string, tone?: "ok" | "error") => void,
): Promise<void> {
  try {
    await bridge.invoke("app_reveal_path", { path });
  } catch (error) {
    notify(errorText(error), "error");
  }
}
