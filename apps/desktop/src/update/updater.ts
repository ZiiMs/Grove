import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent } from "@tauri-apps/plugin-updater";

/** One announced update, with the resource kept alive until it is installed or dismissed. */
export interface PendingUpdate {
  version: string;
  currentVersion: string;
  notes: string;
  date: string | null;
  /** Download and install; `percent` is null while the server does not report a length. */
  install: (onProgress: (percent: number | null) => void) => Promise<void>;
  dismiss: () => Promise<void>;
}

/**
 * Ask the configured endpoint for a newer signed build. Returns null when the running version is
 * current. The caller owns the returned update and must `dismiss()` it if it is not installed.
 */
export async function checkForUpdate(): Promise<PendingUpdate | null> {
  const update = await check();
  if (!update) return null;

  return {
    version: update.version,
    currentVersion: update.currentVersion,
    notes: update.body?.trim() || "No release notes were published for this version.",
    date: update.date ?? null,
    async install(onProgress) {
      let total = 0;
      let received = 0;
      await update.downloadAndInstall((event: DownloadEvent) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          received = 0;
          onProgress(total > 0 ? 0 : null);
        } else if (event.event === "Progress") {
          received += event.data.chunkLength;
          onProgress(total > 0 ? Math.min(100, Math.round((received / total) * 100)) : null);
        } else {
          onProgress(100);
        }
      });
    },
    dismiss: () => update.close(),
  };
}

/** Restart into the newly installed build (the Windows installer may already have exited us). */
export async function restartForUpdate(): Promise<void> {
  await relaunch();
}
