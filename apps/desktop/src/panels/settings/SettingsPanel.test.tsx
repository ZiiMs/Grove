import type { DoctorReport, Settings } from "@grove/contract";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { BridgeProvider } from "../../ipc/context";
import { createFakeBridge, DEFAULT_HARNESSES } from "../../test-utils/fakeBridge";
import { SettingsPanel } from "./index";

const settings: Settings = {
  harnessPaths: { mock: "C:/tools/old-mock.cmd" },
  catalogSources: ["anthropics/skills"],
  theme: "dark",
};

const doctor: DoctorReport = {
  harnesses: [
    {
      id: "mock",
      label: "Mock (builtin)",
      installed: true,
      version: "0.1.0",
      path: "builtin:mock",
      detail: null,
    },
  ],
  tools: [
    {
      id: "node",
      label: "Node.js",
      installed: true,
      version: "v20.11.1",
      path: "C:/Program Files/nodejs/node.exe",
      detail: null,
    },
  ],
  checkedAt: "2025-01-01T00:00:00.000Z",
};

function renderPanel(
  seed: Parameters<typeof createFakeBridge>[1] = {},
): ReturnType<typeof createFakeBridge> {
  const bridge = createFakeBridge({}, { harnesses: DEFAULT_HARNESSES, doctor, ...seed });
  render(
    <BridgeProvider bridge={bridge}>
      <SettingsPanel />
    </BridgeProvider>,
  );
  return bridge;
}

describe("SettingsPanel", () => {
  it("loads settings, saves a harness path, catalog sources and the theme", async () => {
    const bridge = renderPanel({ settings });

    const pathInput = await screen.findByLabelText("Path for mock");
    expect(pathInput).toHaveValue("C:/tools/old-mock.cmd");
    expect(screen.getByLabelText("Theme")).toHaveValue("dark");
    expect(screen.getByLabelText("Catalog sources")).toHaveValue("anthropics/skills");

    await userEvent.clear(pathInput);
    await userEvent.type(pathInput, "C:/tools/new-mock.cmd");
    const sources = screen.getByLabelText("Catalog sources");
    await userEvent.clear(sources);
    await userEvent.type(sources, "acme/skills");
    await userEvent.selectOptions(screen.getByLabelText("Theme"), "light");
    await userEvent.click(screen.getByRole("button", { name: "Save settings" }));

    expect(await screen.findByTestId("settings-save")).toHaveTextContent("Settings saved.");
    const save = bridge.calls.find((call) => call.command === "settings_set");
    expect(save?.args).toEqual({
      patch: {
        harnessPaths: { mock: "C:/tools/new-mock.cmd" },
        catalogSources: ["acme/skills"],
        theme: "light",
      },
    });
    expect(screen.getByLabelText("Path for mock")).toHaveValue("C:/tools/new-mock.cmd");
  });

  it("shows a keychain status chip and never renders a saved token value", async () => {
    const bridge = renderPanel({ secretStatuses: { "github.token": "present" } });

    const chip = await screen.findByTestId("secret-status");
    expect(chip).toHaveTextContent("Saved in keychain");
    expect(chip).toHaveAttribute("data-status", "present");

    const input = screen.getByLabelText("GitHub token");
    await userEvent.type(input, "ghp_never_render_me");
    await userEvent.click(screen.getByRole("button", { name: "Save token" }));

    await waitFor(() => {
      const call = bridge.calls.find((entry) => entry.command === "secret_set");
      expect(call?.args).toEqual({
        key: "github.token",
        value: "ghp_never_render_me",
      });
    });

    // The input is cleared and the value is nowhere in the rendered document.
    await waitFor(() => expect(input).toHaveValue(""));
    expect(screen.queryByDisplayValue("ghp_never_render_me")).not.toBeInTheDocument();
    expect(document.body.textContent).not.toContain("ghp_never_render_me");
    expect(screen.getByTestId("secret-status")).toHaveTextContent("Saved in keychain");
  });

  it("distinguishes an unavailable keychain from a missing token", async () => {
    renderPanel({ secretStatuses: { "github.token": "unavailable" } });

    const chip = await screen.findByTestId("secret-status");
    expect(chip).toHaveTextContent("Keychain unavailable");
    expect(chip).toHaveAttribute("data-status", "unavailable");
  });

  it("refreshes the doctor table on demand", async () => {
    const bridge = createFakeBridge({}, { harnesses: DEFAULT_HARNESSES });
    let current: DoctorReport = doctor;
    bridge.on("doctor_report", () => current);
    render(
      <BridgeProvider bridge={bridge}>
        <SettingsPanel />
      </BridgeProvider>,
    );

    expect(await screen.findByTestId("doctor-mock")).toHaveTextContent("0.1.0");
    expect(screen.getByTestId("doctor-node")).toHaveTextContent("installed");

    current = {
      harnesses: [
        { ...doctor.harnesses[0], version: "0.2.0" },
        {
          id: "claude",
          label: "Claude Code",
          installed: false,
          version: null,
          path: "C:/Tools/claude.exe",
          detail: "configured path does not exist: C:/Tools/claude.exe",
        },
      ],
      tools: doctor.tools,
      checkedAt: "2025-01-01T00:05:00.000Z",
    };

    await userEvent.click(screen.getByRole("button", { name: "Refresh doctor" }));

    expect(await screen.findByText("0.2.0")).toBeInTheDocument();
    const missing = await screen.findByTestId("doctor-claude");
    expect(missing).toHaveTextContent("missing");
    expect(missing).toHaveTextContent("configured path does not exist");
    expect(bridge.invocations.filter((name) => name === "doctor_report")).toHaveLength(2);
  });
});
