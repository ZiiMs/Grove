import type { SpecDoc, SpecGraph, SpecNode } from "@grove/contract";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { GroveBridge } from "../../ipc/bridge";
import { BridgeProvider } from "../../ipc/context";
import { createFakeBridge, type FakeBridge } from "../../test-utils/fakeBridge";
import { SpecsPanel } from ".";

/**
 * jsdom cannot render Monaco (it needs a real layout engine), so the lazy editor is replaced with a
 * plain textarea. The panel still mounts it through `React.lazy`, which proves the editor chunk is
 * only requested when editing starts.
 */
vi.mock("@monaco-editor/react", () => ({
  default: ({
    value,
    onChange,
  }: {
    value?: string;
    onChange?: (value: string | undefined) => void;
  }) => (
    <textarea
      aria-label="Spec editor"
      value={value ?? ""}
      onChange={(event) => onChange?.(event.target.value)}
    />
  ),
}));

const goalNode: SpecNode = {
  id: "grove-goal-and-requirements",
  type: "goal-and-requirements",
  title: "Grove goal",
  status: "active",
  path: "goal.md",
  dependsOn: [],
  references: [],
  implements: [],
  covers: ["grove-architecture"],
};

const architectureNode: SpecNode = {
  id: "grove-architecture",
  type: "architecture-design",
  title: "Grove architecture",
  status: "draft",
  path: "architecture.md",
  parent: "grove-goal-and-requirements",
  dependsOn: ["grove-goal-and-requirements"],
  references: [],
  implements: [],
  covers: [],
};

const graph: SpecGraph = {
  root: "C:/demo/grove",
  nodes: [goalNode, architectureNode],
  validation: { danglingLinks: [], duplicateIds: [], parentCycles: [] },
};

const docs: Record<string, SpecDoc> = {
  "goal.md": {
    node: goalNode,
    body: '# Grove goal\n\nRead the <script>alert("x")</script> intent.\n',
    hash: "hash-goal-1",
  },
  "architecture.md": {
    node: architectureNode,
    body: "# Grove architecture\n\nA Tauri shell over a Rust core.\n",
    hash: "hash-architecture-1",
  },
};

type WriteArgs = { path: string; body: string; expectedHash: string };

function createSpecBridge(
  handlers: {
    index?: () => SpecGraph;
    read?: (path: string) => SpecDoc;
    write?: (args: WriteArgs) => SpecDoc;
  } = {},
): { bridge: FakeBridge; invoke: ReturnType<typeof vi.fn> } {
  const invoke = vi.fn(async (command: string, args?: unknown) => {
    switch (command) {
      case "spec_index":
        return (handlers.index ?? (() => graph))();
      case "spec_read": {
        const { path } = args as { path: string };
        const doc = docs[path];
        if (!doc) throw { code: "not_found", message: `no spec at ${path}` };
        return handlers.read ? handlers.read(path) : doc;
      }
      case "spec_write": {
        const write = args as WriteArgs;
        if (handlers.write) return handlers.write(write);
        return { ...docs[write.path], body: write.body, hash: "hash-after-save" };
      }
      default:
        throw new Error(`unexpected command: ${command}`);
    }
  });
  const bridge = createFakeBridge({ invoke: invoke as unknown as GroveBridge["invoke"] });
  return { bridge, invoke };
}

function renderPanel(bridge: FakeBridge) {
  render(
    <BridgeProvider bridge={bridge}>
      <SpecsPanel />
    </BridgeProvider>,
  );
}

describe("SpecsPanel", () => {
  it("renders the spec tree grouped by type with status", async () => {
    const { bridge, invoke } = createSpecBridge();
    renderPanel(bridge);

    expect(await screen.findByRole("heading", { name: "goal-and-requirements" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "architecture-design" })).toBeInTheDocument();

    const goalRow = screen.getByRole("button", { name: /Grove goal/ });
    expect(within(goalRow).getByText("active")).toBeInTheDocument();
    expect(within(goalRow).getByText("grove-goal-and-requirements")).toBeInTheDocument();

    const architectureRow = screen.getByRole("button", { name: /Grove architecture/ });
    expect(within(architectureRow).getByText("draft")).toBeInTheDocument();

    expect(screen.getByText("C:/demo/grove")).toBeInTheDocument();
    expect(invoke.mock.calls.some(([command]) => command === "spec_index")).toBe(true);
  });

  it("opens a doc, renders its markdown safely, and walks a link to another spec", async () => {
    const { bridge, invoke } = createSpecBridge();
    renderPanel(bridge);

    await userEvent.click(await screen.findByRole("button", { name: /Grove goal/ }));

    const reader = await screen.findByTestId("spec-reader");
    expect(reader.querySelector("h1")).toHaveTextContent("Grove goal");
    expect(reader.querySelector("script")).toBeNull();
    expect(reader.textContent).toContain('<script>alert("x")</script>');
    expect(invoke).toHaveBeenCalledWith("spec_read", { path: "goal.md" });

    // The goal doc lists the specs that point at it, so the graph is walkable in both directions.
    await userEvent.click(screen.getAllByRole("button", { name: "Open Grove architecture" })[0]);
    expect(
      await screen.findByRole("heading", { level: 2, name: "Grove architecture" }),
    ).toBeInTheDocument();
  });

  it("lazy-loads the editor only when editing and saves with the expected hash", async () => {
    const { bridge, invoke } = createSpecBridge();
    renderPanel(bridge);

    await userEvent.click(await screen.findByRole("button", { name: /Grove goal/ }));
    await screen.findByTestId("spec-reader");
    expect(screen.queryByLabelText("Spec editor")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Edit" }));
    const editor = await screen.findByLabelText("Spec editor");
    fireEvent.change(editor, { target: { value: "# Updated goal\n" } });

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("spec_write", {
        path: "goal.md",
        body: "# Updated goal\n",
        expectedHash: "hash-goal-1",
      });
    });
    expect(await screen.findByRole("heading", { name: "Updated goal" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Spec editor")).not.toBeInTheDocument();
  });

  it("surfaces a conflict and lets the user reload the newer content", async () => {
    let reads = 0;
    let onDisk = docs["goal.md"];
    const { bridge } = createSpecBridge({
      read: () => {
        reads += 1;
        return onDisk;
      },
      write: () => {
        throw { code: "conflict", message: "spec changed on disk" };
      },
    });
    renderPanel(bridge);

    await userEvent.click(await screen.findByRole("button", { name: /Grove goal/ }));
    await screen.findByTestId("spec-reader");
    await userEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(await screen.findByLabelText("Spec editor"), {
      target: { value: "# My stale edit\n" },
    });
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/changed on disk/i);

    onDisk = {
      node: goalNode,
      body: "# Someone else's edit\n",
      hash: "hash-goal-2",
    };
    await userEvent.click(within(alert).getByRole("button", { name: "Reload" }));

    expect(await screen.findByRole("heading", { name: "Someone else's edit" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(reads).toBeGreaterThanOrEqual(2);
  });
});
