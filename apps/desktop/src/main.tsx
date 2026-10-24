import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { createTauriBridge } from "./ipc/bridge";
import { BridgeProvider } from "./ipc/context";
import "./styles/tokens.css";
import "./styles/app.css";

const bridge = createTauriBridge();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <BridgeProvider bridge={bridge}>
      <App />
    </BridgeProvider>
  </React.StrictMode>,
);
