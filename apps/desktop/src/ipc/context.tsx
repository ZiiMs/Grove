import { createContext, useContext, type ReactNode } from "react";
import type { GroveBridge } from "./bridge";

const BridgeContext = createContext<GroveBridge | null>(null);

export function BridgeProvider({ bridge, children }: { bridge: GroveBridge; children: ReactNode }) {
  return <BridgeContext.Provider value={bridge}>{children}</BridgeContext.Provider>;
}

export function useBridge(): GroveBridge {
  const bridge = useContext(BridgeContext);
  if (!bridge) {
    throw new Error("useBridge must be used inside <BridgeProvider>");
  }
  return bridge;
}
