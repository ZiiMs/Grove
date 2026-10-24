import type { InstallResult, SkillCatalog, SkillPreview, SkillTarget } from "@grove/contract";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { CommandArgs, CommandMap, CommandName } from "../../ipc/commands";
import { BridgeProvider } from "../../ipc/context";
import { createFakeBridge, type FakeBridge } from "../../test-utils/fakeBridge";
import { SkillsPanel } from "./index";

const pdfEntry = {
  id: "anthropics/skills::skills/pdf",
  name: "pdf",
  source: "anthropics/skills",
  path: "skills/pdf",
};
const slidesEntry = {
  id: "anthropics/skills::skills/slides",
  name: "slides",
  source: "anthropics/skills",
  path: "skills/slides",
};

const catalog: SkillCatalog = {
  entries: [pdfEntry, slidesEntry],
  fetchedAt: "2026-10-24T18:00:00Z",
  fromCache: false,
  offline: false,
};

const targets: SkillTarget[] = [
  {
    id: "claude",
    label: "Claude Code",
    path: "C:/Users/demo/.claude/skills",
    exists: true,
  },
  {
    id: "agents",
    label: "Project .agents/skills",
    path: "C:/Users/demo/project/.agents/skills",
    exists: false,
  },
];

const preview: SkillPreview = {
  entry: pdfEntry,
  frontmatter: { name: "pdf", description: "Create and inspect PDF files" },
  body: "# PDF\n\nUse this skill to work with PDF files.",
};

const installed: InstallResult = {
  skill: "pdf",
  source: "anthropics/skills",
  target: "claude",
  path: "C:/Users/demo/.claude/skills/pdf",
  overwritten: false,
  installedAt: "2026-10-24T18:00:00Z",
};

interface Handlers {
  catalog?: SkillCatalog;
  targets?: SkillTarget[];
  preview?: SkillPreview;
  install?: InstallResult;
  installError?: { code: string; message: string };
}

function createSkillBridge(handlers: Handlers) {
  const calls: { command: string; args: unknown }[] = [];
  const bridge: FakeBridge = createFakeBridge({
    async invoke<K extends CommandName>(
      command: K,
      args?: CommandArgs<K>,
    ): Promise<CommandMap[K]["result"]> {
      calls.push({ command, args });
      switch (command) {
        case "skill_catalog":
          return (handlers.catalog ?? catalog) as CommandMap[K]["result"];
        case "skill_targets":
          return (handlers.targets ?? targets) as CommandMap[K]["result"];
        case "skill_preview":
          return (handlers.preview ?? preview) as CommandMap[K]["result"];
        case "skill_install":
          if (handlers.installError) throw handlers.installError;
          return (handlers.install ?? installed) as CommandMap[K]["result"];
        default:
          throw new Error(`FakeBridge has no handler for command: ${command as string}`);
      }
    },
  });
  return { bridge, calls };
}

function renderPanel(bridge: FakeBridge) {
  render(
    <BridgeProvider bridge={bridge}>
      <SkillsPanel />
    </BridgeProvider>,
  );
}

describe("SkillsPanel", () => {
  it("browses the catalog and filters by search text", async () => {
    const { bridge } = createSkillBridge({});
    renderPanel(bridge);

    expect(await screen.findByRole("button", { name: /pdf/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /slides/i })).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText("Search skills"), "sli");

    expect(screen.queryByRole("button", { name: /^pdf/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /slides/i })).toBeInTheDocument();
  });

  it("previews a skill with its frontmatter and body", async () => {
    const { bridge, calls } = createSkillBridge({});
    renderPanel(bridge);

    await userEvent.click(await screen.findByRole("button", { name: /pdf/i }));

    expect(await screen.findByTestId("skill-preview")).toBeInTheDocument();
    expect(screen.getByText("Create and inspect PDF files")).toBeInTheDocument();
    expect(screen.getByText(/Use this skill to work with PDF files/)).toBeInTheDocument();
    const previewCall = calls.find((call) => call.command === "skill_preview");
    expect(previewCall?.args).toEqual({ source: "anthropics/skills", path: "skills/pdf" });
  });

  it("installs into the selected destination and reports success", async () => {
    const { bridge, calls } = createSkillBridge({});
    renderPanel(bridge);

    await userEvent.click(await screen.findByRole("button", { name: /pdf/i }));
    await screen.findByTestId("skill-preview");
    await userEvent.selectOptions(screen.getByLabelText("Destination"), "agents");
    await userEvent.click(screen.getByRole("button", { name: "Install skill" }));

    expect(await screen.findByTestId("install-success")).toHaveTextContent(/Installed pdf/);
    expect(screen.getByTestId("install-success")).toHaveTextContent(/new/);
    const installCall = calls.find((call) => call.command === "skill_install");
    expect(installCall?.args).toEqual({
      source: "anthropics/skills",
      path: "skills/pdf",
      target: "agents",
    });
  });

  it("shows a failure when the install command rejects", async () => {
    const { bridge } = createSkillBridge({
      installError: { code: "io", message: "disk full" },
    });
    renderPanel(bridge);

    await userEvent.click(await screen.findByRole("button", { name: /pdf/i }));
    await screen.findByTestId("skill-preview");
    await userEvent.click(screen.getByRole("button", { name: "Install skill" }));

    expect(await screen.findByTestId("install-error")).toHaveTextContent(/disk full/);
  });

  it("stays usable when the catalog is served offline from cache", async () => {
    const { bridge } = createSkillBridge({
      catalog: { ...catalog, fromCache: true, offline: true },
    });
    renderPanel(bridge);

    expect(await screen.findByTestId("catalog-offline")).toHaveTextContent(/cached/i);
    expect(screen.getByRole("button", { name: /pdf/i })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /pdf/i }));
    expect(await screen.findByTestId("skill-preview")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Install skill" })).toBeEnabled();
  });

  it("shows a catalog error with a retry control when there is no cache", async () => {
    const bridge = createFakeBridge({
      async invoke<K extends CommandName>(): Promise<CommandMap[K]["result"]> {
        throw { code: "unavailable", message: "network is down" };
      },
    });
    renderPanel(bridge);

    expect(await screen.findByTestId("catalog-error")).toHaveTextContent(/network is down/);
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });
});
