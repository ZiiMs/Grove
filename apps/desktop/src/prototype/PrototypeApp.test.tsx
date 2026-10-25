/**
 * PROTOTYPE — throwaway smoke test. Catches a broken shell without anyone having to open the window.
 */

import { fireEvent, render, screen, within } from "@testing-library/react";
import { PrototypeApp } from "./PrototypeApp";

function badge() {
  return screen.getByTestId("variant-badge").textContent ?? "";
}

function goTo(section: string) {
  fireEvent.change(screen.getByLabelText("Prototype section"), { target: { value: section } });
}

describe("prototype surface", () => {
  it("opens on the theme guide", () => {
    render(<PrototypeApp />);
    expect(screen.getByTestId("prototype-root")).toBeInTheDocument();
    for (const name of ["Lattice", "Ember", "Console", "Paperwork"]) {
      expect(screen.getAllByText(name).length).toBeGreaterThan(0);
    }
    expect(screen.getByTestId("prototype-switcher")).toBeInTheDocument();
  });

  it("renders the project tree with chats, projects, worktrees and sessions", () => {
    render(<PrototypeApp />);
    const nav = within(screen.getByLabelText("Projects"));
    expect(nav.getByText("Chats")).toBeInTheDocument();
    expect(nav.getByText("Projects")).toBeInTheDocument();
    expect(nav.getByText("grove")).toBeInTheDocument();
    expect(nav.getByText("workspace-3")).toBeInTheDocument();
    expect(nav.getByText("ACP adapter behind HarnessAdapter")).toBeInTheDocument();
    expect(nav.getByLabelText("Add project")).toBeInTheDocument();
    expect(nav.getByLabelText("New chat without a project")).toBeInTheDocument();
    expect(nav.getByLabelText("Skill catalog")).toBeInTheDocument();
    expect(nav.getByLabelText("MCP servers")).toBeInTheDocument();
    expect(nav.getByLabelText("Settings")).toBeInTheDocument();
  });

  it("keeps chat in the centre with a slim header and no panel tabs", () => {
    render(<PrototypeApp />);
    goTo("sessions");
    expect(badge()).toMatch(/^A · /);
    // Slim chat header survives; the breadcrumb row and tab strip are gone.
    expect(screen.getByTestId("chat-title")).toHaveTextContent("ACP adapter behind HarnessAdapter");
    expect(screen.queryByTestId("tab-sessions")).not.toBeInTheDocument();
  });

  it("opens settings, skills and mcp in the modal", () => {
    render(<PrototypeApp />);
    const nav = within(screen.getByLabelText("Projects"));

    fireEvent.click(nav.getByLabelText("Settings"));
    expect(screen.getByTestId("settings-modal")).toBeInTheDocument();
    expect(screen.getAllByText("General").length).toBeGreaterThan(0);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByTestId("settings-modal")).not.toBeInTheDocument();

    fireEvent.click(nav.getByLabelText("Skill catalog"));
    expect(screen.getByTestId("settings-modal")).toBeInTheDocument();
    // Skills opens straight onto the catalog section (variant B, chosen).
    expect(screen.getByText("Catalogs")).toBeInTheDocument();

    fireEvent.click(nav.getByLabelText("MCP servers"));
    expect(screen.getByTestId("settings-modal")).toBeInTheDocument();
    expect(screen.getByText("Servers")).toBeInTheDocument();
  });

  it("collapses and expands the sidepanel", () => {
    render(<PrototypeApp />);
    fireEvent.click(screen.getByTitle("Toggle sidepanel"));
    expect(screen.getAllByLabelText("Side panel (collapsed)").length).toBe(1);
    fireEvent.click(screen.getByTitle("Toggle sidepanel"));
    expect(screen.getAllByLabelText("Side panel").length).toBe(1);
  });

  it("cycles the layouts of the centre and side specimens", () => {
    render(<PrototypeApp />);
    for (const section of ["sessions", "plan", "specs", "git"]) {
      goTo(section);
      const first = badge();
      expect(first).toMatch(/^[ABC] · /);
      fireEvent.click(screen.getByLabelText("Next"));
      expect(badge()).not.toBe(first);
      fireEvent.click(screen.getByLabelText("Next"));
      expect(badge()).toMatch(/^[ABC] · /);
    }
  });

  it("cycles theme presets on sections without variants", () => {
    render(<PrototypeApp />);
    const root = screen.getByTestId("prototype-root");
    // Ember is the chosen default (see defaultTheme) at 0px radius.
    expect(root.style.getPropertyValue("--bg")).toBe("#12100d");
    expect(root.style.getPropertyValue("--radius-md")).toBe("0px");

    fireEvent.click(screen.getByLabelText("Next"));
    expect(root.style.getPropertyValue("--bg")).toBe("#070b12");
  });
});
