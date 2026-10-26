import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { BridgeProvider } from "../ipc/context";
import { useSkillsStore } from "../state/skillsStore";
import { useUiStore } from "../state/uiStore";
import { createFakeBridge, DEFAULT_SETTINGS, type FakeBridgeSeed } from "../test-utils/fakeBridge";
import { SkillsPane } from "./SkillsPane";

const PROJECT_ROOT = "C:/projects/grove";

const SEED = {
  settings: { ...DEFAULT_SETTINGS, catalogSources: ["acme/skills"] },
  skillCatalog: {
    entries: [
      { id: "acme/skills::skills/pdf", name: "pdf", source: "acme/skills", path: "skills/pdf" },
      { id: "acme/skills::skills/docs", name: "docs", source: "acme/skills", path: "skills/docs" },
    ],
    fetchedAt: "2026-10-24T18:00:00Z",
    fromCache: false,
    offline: false,
  },
  skillTargets: [
    {
      id: "user",
      label: "User .agents/skills",
      path: "C:/Users/demo/.agents/skills",
      exists: true,
    },
    {
      id: "agents",
      label: "Project .agents/skills",
      path: `${PROJECT_ROOT}/.agents/skills`,
      exists: true,
    },
  ],
  installs: [
    {
      skill: "docs",
      source: "acme/skills",
      target: "agents",
      path: `${PROJECT_ROOT}/.agents/skills/docs`,
      installedAt: "2025-01-01T00:00:00Z",
    },
  ],
};

function renderPane(seed: FakeBridgeSeed = SEED) {
  const bridge = createFakeBridge({}, seed);
  render(
    <BridgeProvider bridge={bridge}>
      <SkillsPane projectRoot={PROJECT_ROOT} />
    </BridgeProvider>,
  );
  return bridge;
}

beforeEach(() => {
  cleanup();
  useUiStore.getState().reset();
  useSkillsStore.setState({
    catalog: null,
    installs: [],
    inventory: [],
    sources: [],
    targets: [],
    preview: null,
    selectedId: null,
    query: "",
    loading: false,
    previewLoading: false,
    installing: null,
    refreshing: false,
    error: null,
  });
});

describe("SkillsPane", () => {
  it("selects every visible skill and installs them to the chosen scope", async () => {
    const bridge = renderPane();
    expect(await screen.findByText("2 skills")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("checkbox", { name: "Select all visible skills" }));
    const install = screen.getByRole("button", { name: "Install 2" });
    expect(install).toBeEnabled();
    await userEvent.click(install);

    const dialog = await screen.findByRole("dialog", { name: "Install 2 skills" });
    const options = within(dialog);
    const target = options.getByRole("combobox", { name: "Install to" });
    await userEvent.click(target);
    await userEvent.click(options.getByRole("option", { name: "User .agents/skills" }));
    await userEvent.click(options.getByRole("button", { name: "Install" }));

    await waitFor(() => {
      const calls = bridge.calls.filter((call) => call.command === "skill_install");
      expect(calls).toHaveLength(2);
      expect(calls.map((call) => (call.args as { target: string }).target)).toEqual([
        "user",
        "user",
      ]);
    });
    expect(screen.getByRole("button", { name: "Install" })).toBeDisabled();
  });

  it("offers only the global location when there is no project", async () => {
    const bridge = createFakeBridge({}, SEED);
    render(
      <BridgeProvider bridge={bridge}>
        <SkillsPane />
      </BridgeProvider>,
    );
    await screen.findByText("2 skills");

    await userEvent.click(screen.getByRole("checkbox", { name: "Select pdf" }));
    await userEvent.click(screen.getByRole("button", { name: "Install 1" }));

    const dialog = await screen.findByRole("dialog", { name: "Install pdf" });
    const target = within(dialog).getByRole("combobox", { name: "Install to" });
    await userEvent.click(target);
    expect(within(dialog).getByRole("option", { name: "User .agents/skills" })).toBeInTheDocument();
    expect(
      within(dialog).queryByRole("option", { name: "Project .agents/skills" }),
    ).not.toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("option", { name: "User .agents/skills" }));

    await userEvent.click(within(dialog).getByRole("button", { name: "Install" }));

    await waitFor(() => {
      const calls = bridge.calls.filter((call) => call.command === "skill_install");
      expect(calls).toHaveLength(1);
      expect((calls[0].args as { target: string }).target).toBe("user");
    });
  });

  it("removes a skill already on disk even without an install record", async () => {
    const bridge = renderPane({
      ...SEED,
      installs: [],
      skillInventory: [
        {
          name: "pdf",
          target: "agents",
          path: `${PROJECT_ROOT}/.agents/skills/pdf`,
        },
      ],
    });
    expect(await screen.findByText("1 installed")).toBeInTheDocument();

    const detail = screen.getByLabelText("Selected skill");
    expect(await within(detail).findByText("Found in Project .agents/skills")).toBeInTheDocument();
    await userEvent.click(within(detail).getByRole("button", { name: "Remove pdf" }));

    await waitFor(() => {
      const calls = bridge.calls.filter((call) => call.command === "skill_remove");
      expect(calls).toHaveLength(1);
      expect((calls[0].args as { path: string }).path).toBe(
        `${PROJECT_ROOT}/.agents/skills/pdf`,
      );
    });
    expect(await screen.findByText("0 installed")).toBeInTheDocument();
    expect(
      within(detail).queryByRole("button", { name: "Remove pdf" }),
    ).not.toBeInTheDocument();
  });

  it("uninstalls every installed skill after confirmation", async () => {
    const bridge = renderPane();
    expect(await screen.findByText("1 installed")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Uninstall all" }));
    const dialog = await screen.findByRole("dialog", { name: "Uninstall all skills?" });
    expect(within(dialog).getByText(/1 installed skill copy/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Uninstall all" }));

    await waitFor(() => {
      const calls = bridge.calls.filter((call) => call.command === "skill_remove");
      expect(calls.map((call) => (call.args as { path: string }).path)).toEqual([
        "C:/projects/grove/.agents/skills/docs",
      ]);
    });
    expect(await screen.findByText("0 installed")).toBeInTheDocument();
  });

  it("refreshes the catalog from the header", async () => {
    const bridge = renderPane();
    expect(await screen.findByText("2 skills")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Refresh catalog" }));
    await waitFor(() => {
      const refreshes = bridge.calls.filter(
        (call) =>
          call.command === "skill_catalog" &&
          (call.args as { refresh?: boolean }).refresh === true,
      );
      expect(refreshes).toHaveLength(1);
    });
  });

  it("adds a GitHub catalog from the plus row", async () => {
    const bridge = renderPane();
    expect(await screen.findByText("2 skills")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /Add catalog/ }));
    const dialog = await screen.findByRole("dialog", { name: "Add catalog" });
    await userEvent.type(
      within(dialog).getByLabelText("GitHub repository"),
      "https://github.com/acme/new-skills",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Add catalog" }));

    await waitFor(() => {
      const calls = bridge.calls.filter((call) => call.command === "settings_set");
      expect(calls).toHaveLength(1);
      expect(calls[0].args).toMatchObject({
        patch: { catalogSources: ["acme/skills", "acme/new-skills"] },
      });
    });
    expect(
      await screen.findByRole("button", { name: /acme\/new-skills/ }),
    ).toBeInTheDocument();
    expect(await screen.findByText("0 skills")).toBeInTheDocument();
    expect(screen.getByText("This catalog has no skills.")).toBeInTheDocument();
  });
});
