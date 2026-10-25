import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { createTauriBridge } from "./ipc/bridge";
import { BridgeProvider } from "./ipc/context";
import { PrototypeApp } from "./prototype/PrototypeApp";
import "./styles/tokens.css";
import "./styles/app.css";

// PROTOTYPE gate — throwaway, see src/prototype/. `?prototype` swaps the whole shell for the
// UI exploration surface; nothing in the prototype touches the live panels or tokens.
const isPrototype = new URLSearchParams(window.location.search).has("prototype");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isPrototype ? (
      <PrototypeApp />
    ) : (
      <BridgeProvider bridge={createTauriBridge()}>
        <App />
      </BridgeProvider>
    )}
  </React.StrictMode>,
);
