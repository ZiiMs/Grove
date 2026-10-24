import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import App from "./App";
import { BridgeProvider } from "./ipc/context";
import { panels } from "./panels/registry";
import { createFakeBridge } from "./test-utils/fakeBridge";

function renderApp() {
  const bridge = createFakeBridge();
  render(
    <BridgeProvider bridge={bridge}>
      <App />
    </BridgeProvider>,
  );
  return bridge;
}

async function renderAppReady() {
  const bridge = renderApp();
  await screen.findByText(/Grove v0\.1\.0/);
  return bridge;
}

describe("shell", () => {
  it("renders one navigation entry per panel", async () => {
    await renderAppReady();
    expect(screen.getByTestId("app-shell")).toBeInTheDocument();
    expect(panels).toHaveLength(7);
    for (const panel of panels) {
      expect(screen.getByRole("button", { name: new RegExp(panel.title) })).toBeInTheDocument();
    }
  });

  it("mounts only the active panel and switches on click", async () => {
    await renderAppReady();
    expect(screen.getByTestId("panel-sessions")).toBeInTheDocument();
    expect(screen.queryByTestId("panel-skills")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /Skills/ }));

    expect(screen.getByTestId("panel-skills")).toBeInTheDocument();
    expect(screen.queryByTestId("panel-sessions")).not.toBeInTheDocument();
  });

  it("shows the core version reported by app_info", async () => {
    const bridge = await renderAppReady();
    expect(screen.getByText(/Grove v0\.1\.0/)).toBeInTheDocument();
    expect(bridge.invocations).toContain("app_info");
  });
});
