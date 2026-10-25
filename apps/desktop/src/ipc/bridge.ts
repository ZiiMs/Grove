import type { CommandArgs, CommandMap, CommandName } from "./commands";

export type UnlistenFn = () => void;
export type GroveEventName = `grove://${string}`;
export type RawEventHandler = (payload: unknown) => void;

/**
 * The only door between the webview and the core. Panels and stores take a bridge; they never import
 * `@tauri-apps/*` themselves, which is what lets every test render them under `FakeBridge`.
 */
export interface GroveBridge {
  invoke<K extends CommandName>(command: K, args?: CommandArgs<K>): Promise<CommandMap[K]["result"]>;
  listen(event: GroveEventName, handler: RawEventHandler): Promise<UnlistenFn>;
}

/** The real bridge. Tauri is imported lazily so tests never load it. */
export function createTauriBridge(): GroveBridge {
  return {
    async invoke<K extends CommandName>(
      command: K,
      args?: CommandArgs<K>,
    ): Promise<CommandMap[K]["result"]> {
      const { invoke } = await import("@tauri-apps/api/core");
      return (await invoke(command as string, args as Record<string, unknown>)) as CommandMap[K]["result"];
    },
    async listen(event: GroveEventName, handler: RawEventHandler): Promise<UnlistenFn> {
      const { listen } = await import("@tauri-apps/api/event");
      return listen(event, (payload) => handler(payload.payload));
    },
  };
}
