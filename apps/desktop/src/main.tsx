import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { createTauriBridge } from "./ipc/bridge";
import { BridgeProvider } from "./ipc/context";
import "./styles/fonts.css";
import "./styles/themes.css";
import "./styles/design.css";
import "./styles/extras.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <BridgeProvider bridge={createTauriBridge()}>
      <App />
    </BridgeProvider>
  </React.StrictMode>,
);
