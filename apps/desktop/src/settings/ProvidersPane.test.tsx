import type { ProviderStatus } from "@grove/contract";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { BridgeProvider } from "../ipc/context";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import {
  DEFAULT_PROVIDERS,
  DEFAULT_SETTINGS,
  FAKE_PROVIDER_ROSTER,
  createFakeBridge,
} from "../test-utils/fakeBridge";
import { ProvidersPane } from "./ProvidersPane";

const ROWS = FAKE_PROVIDER_ROSTER;

function renderPane(providerStatuses: ProviderStatus[] = ROWS, defaultProvider = "anthropic") {
  const bridge = createFakeBridge(
    {},
    {
      providers: DEFAULT_PROVIDERS,
      providerStatuses,
      settings: { ...DEFAULT_SETTINGS, defaultProvider },
    },
  );
  useWorkspaceStore.setState({
    prefs: { ...useWorkspaceStore.getState().prefs, defaultProvider },
  });
  render(
    <BridgeProvider bridge={bridge}>
      <ProvidersPane />
    </BridgeProvider>,
  );
  return bridge;
}

beforeEach(() => {
  cleanup();
  useSessionStore.getState().reset();
  useUiStore.getState().reset();
  useWorkspaceStore.getState().reset();
});

describe("ProvidersPane", () => {
  it("shows a card for every connected provider and one card to connect another", async () => {
    renderPane();

    expect(await screen.findByRole("button", { name: /Anthropic/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /OpenAI/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Google/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Connect provider/ })).toBeInTheDocument();
    expect(screen.getByText("2 connected · 5 models")).toBeInTheDocument();
    expect(screen.getByText(/2 of 4 connected/)).toBeInTheDocument();
  });

  it("catalogues only the providers this build ships and labels what is already connected", async () => {
    renderPane();
    await userEvent.click(await screen.findByRole("button", { name: /Connect provider/ }));

    const dialog = screen.getByRole("dialog", { name: "Add a provider" });
    const catalogue = within(dialog);
    expect(catalogue.getByText("4 of 4 providers")).toBeInTheDocument();
    expect(catalogue.getByText("Available to connect")).toBeInTheDocument();
    expect(catalogue.getByRole("button", { name: /Anthropic/ })).toBeDisabled();
    expect(catalogue.getAllByText("connected")).toHaveLength(2);
    expect(catalogue.getByRole("button", { name: /Google/ })).toHaveAccessibleName(
      /Add an API key/,
    );
    expect(catalogue.queryByText(/Mistral|Ollama|ACP/)).not.toBeInTheDocument();
  });

  it("filters the catalogue by name, model or environment variable, and says when nothing matches", async () => {
    renderPane();
    await userEvent.click(await screen.findByRole("button", { name: /Connect provider/ }));
    const search = screen.getByLabelText("Search providers");
    const catalogue = within(screen.getByRole("dialog", { name: "Add a provider" }));

    await userEvent.type(search, "gpt");
    expect(catalogue.getByRole("button", { name: /OpenAI/ })).toBeInTheDocument();
    expect(catalogue.getByRole("button", { name: /OpenRouter/ })).toBeInTheDocument();
    expect(catalogue.queryByRole("button", { name: /Google/ })).not.toBeInTheDocument();

    await userEvent.clear(search);
    await userEvent.type(search, "GEMINI_API_KEY");
    expect(catalogue.getByRole("button", { name: /Google/ })).toBeInTheDocument();

    await userEvent.clear(search);
    await userEvent.type(search, "mistral");
    expect(catalogue.getByText("No providers match")).toBeInTheDocument();
  });

  it("connects a provider from the key step, then closes and shows its card", async () => {
    const bridge = renderPane();
    await userEvent.click(await screen.findByRole("button", { name: /Connect provider/ }));
    await userEvent.click(screen.getByRole("button", { name: /Google/ }));

    const dialog = screen.getByRole("dialog", { name: "Connect Google" });
    expect(document.activeElement).toBe(within(dialog).getByLabelText("Google API key"));

    await userEvent.type(within(dialog).getByLabelText("Google API key"), "AIzaSyExampleKey123");
    await userEvent.click(within(dialog).getByRole("button", { name: "Connect" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(bridge.calls).toContainEqual({
      command: "provider_key_set",
      args: { provider: "google", key: "AIzaSyExampleKey123" },
    });
    expect(useUiStore.getState().toast?.message).toBe("Google key saved to the keychain");
    expect(await screen.findByRole("button", { name: /Google/ })).toBeInTheDocument();
  });

  it("keeps a visibly broken key in the dialog instead of calling the core", async () => {
    const bridge = renderPane();
    await userEvent.click(await screen.findByRole("button", { name: /Connect provider/ }));
    await userEvent.click(screen.getByRole("button", { name: /OpenRouter/ }));

    const key = screen.getByLabelText("OpenRouter API key");
    await userEvent.type(key, "short");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(screen.getByText("That key looks too short — paste the whole thing.")).toBeInTheDocument();

    await userEvent.clear(key);
    await userEvent.type(key, "sk-or-v1-with a space");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(screen.getByText("That key has a space or line break in it — paste it again.")).toBeInTheDocument();

    expect(bridge.calls.some((call) => call.command === "provider_key_set")).toBe(false);
  });

  it("manages a connected provider: tests the key, makes it default, then disconnects", async () => {
    const bridge = renderPane(ROWS, "openai");
    await userEvent.click(await screen.findByRole("button", { name: /Anthropic/ }));

    const dialog = screen.getByRole("dialog", { name: "Manage Anthropic" });
    const manage = within(dialog);
    expect(manage.getByText("in the OS keychain")).toBeInTheDocument();
    expect(manage.getByText("claude-sonnet-4-5")).toBeInTheDocument();
    expect(manage.getByRole("link", { name: "https://console.anthropic.com/settings/keys" })).toHaveAttribute(
      "href",
      "https://console.anthropic.com/settings/keys",
    );

    await userEvent.click(manage.getByRole("button", { name: "Test key" }));
    expect(await manage.findByText("3 models reachable")).toBeInTheDocument();

    await userEvent.click(manage.getByRole("button", { name: "Make default" }));
    await waitFor(() =>
      expect(bridge.calls).toContainEqual({
        command: "settings_set",
        args: { patch: { defaultProvider: "anthropic" } },
      }),
    );
    expect(useUiStore.getState().toast?.message).toBe("Anthropic is the default provider");

    await userEvent.click(manage.getByRole("button", { name: "Disconnect" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(bridge.calls).toContainEqual({
      command: "provider_key_delete",
      args: { provider: "anthropic" },
    });
    expect(screen.queryByRole("button", { name: /Anthropic/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Connect provider/ })).toBeInTheDocument();
  });

  it("marks the card rejected when the vendor refuses the key, and clears it on a new key", async () => {
    const bridge = renderPane();
    bridge.on("provider_models", () => {
      throw { code: "unavailable", message: "HTTP 401: invalid x-api-key" };
    });

    await userEvent.click(await screen.findByRole("button", { name: /Anthropic/ }));
    const manage = within(screen.getByRole("dialog", { name: "Manage Anthropic" }));
    await userEvent.click(manage.getByRole("button", { name: "Test key" }));
    expect(
      await manage.findByText("HTTP 401: invalid x-api-key (unavailable)"),
    ).toBeInTheDocument();
    expect(useUiStore.getState().toast).toEqual({
      message: "HTTP 401: invalid x-api-key (unavailable)",
      tone: "error",
    });

    await userEvent.click(manage.getByRole("button", { name: "Done" }));
    const rejected = await screen.findByRole("button", { name: /Key rejected/ });
    await userEvent.click(rejected);
    await userEvent.click(screen.getByRole("button", { name: "Replace key" }));

    const dialog = screen.getByRole("dialog", { name: "Replace Anthropic key" });
    await userEvent.type(
      within(dialog).getByLabelText("Anthropic API key"),
      "sk-ant-api03-replacement{enter}",
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: /Anthropic.*Connected/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Key rejected/ })).not.toBeInTheDocument();
  });

  it("starts with only the connect card when no credential is configured", async () => {
    const unconfigured = FAKE_PROVIDER_ROSTER.map((row) => ({
      ...row,
      configured: false,
      source: null,
      detail: "Add an API key.",
    }));
    const bridge = renderPane(unconfigured);

    expect(await screen.findByRole("button", { name: /Connect provider/ })).toBeInTheDocument();
    expect(screen.getByText("0 connected")).toBeInTheDocument();
    expect(screen.getByText("0 of 4 connected · 4 available to add")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Google/ })).not.toBeInTheDocument();
    expect(bridge.invocations).toContain("provider_statuses");
  });

  it("backs out of the key step and closes the catalogue without calling the core", async () => {
    const bridge = renderPane();
    await userEvent.click(await screen.findByRole("button", { name: /Connect provider/ }));
    await userEvent.click(screen.getByRole("button", { name: /Google/ }));
    await userEvent.type(screen.getByLabelText("Google API key"), "AIzaSyExampleKey123");

    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("dialog", { name: "Add a provider" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(bridge.calls.some((call) => call.command === "provider_key_set")).toBe(false);
  });

  it("keeps the dialog open and reports the core's message when the key cannot be stored", async () => {
    const bridge = renderPane();
    bridge.on("provider_key_set", () => {
      throw { code: "unavailable", message: "the keychain is locked" };
    });

    await userEvent.click(await screen.findByRole("button", { name: /Connect provider/ }));
    await userEvent.click(screen.getByRole("button", { name: /Google/ }));
    await userEvent.type(screen.getByLabelText("Google API key"), "AIzaSyExampleKey123");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));

    expect(await screen.findByText("the keychain is locked (unavailable)")).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Connect Google" })).toBeInTheDocument();
    expect(useUiStore.getState().toast).toBeNull();
  });

  it("re-keys a connected provider from the manage dialog, submitting with Enter", async () => {
    const bridge = renderPane();
    await userEvent.click(await screen.findByRole("button", { name: /Anthropic/ }));
    await userEvent.click(screen.getByRole("button", { name: "Replace key" }));

    const dialog = screen.getByRole("dialog", { name: "Replace Anthropic key" });
    await userEvent.type(
      within(dialog).getByLabelText("Anthropic API key"),
      "sk-ant-api03-replacement{enter}",
    );

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(bridge.calls).toContainEqual({
      command: "provider_key_set",
      args: { provider: "anthropic", key: "sk-ant-api03-replacement" },
    });
  });

  it("hands the default to another connected provider when the default is disconnected", async () => {
    const bridge = renderPane(ROWS, "anthropic");
    await userEvent.click(await screen.findByRole("button", { name: /Anthropic/ }));
    await userEvent.click(screen.getByRole("button", { name: "Disconnect" }));

    await waitFor(() =>
      expect(bridge.calls).toContainEqual({
        command: "settings_set",
        args: { patch: { defaultProvider: "openai" } },
      }),
    );
    expect(useUiStore.getState().toast?.message).toBe(
      "Anthropic key removed · OpenAI is now the default",
    );
    expect(screen.queryByRole("button", { name: /Anthropic/ })).not.toBeInTheDocument();
  });

  it("does not call a network failure a rejected key", async () => {
    const bridge = renderPane();
    bridge.on("provider_models", () => {
      throw { code: "invalid", message: "cannot reach Anthropic" };
    });

    await userEvent.click(await screen.findByRole("button", { name: /Anthropic/ }));
    const manage = within(screen.getByRole("dialog", { name: "Manage Anthropic" }));
    await userEvent.click(manage.getByRole("button", { name: "Test key" }));
    expect(await manage.findByText("cannot reach Anthropic (invalid)")).toBeInTheDocument();
    await userEvent.click(manage.getByRole("button", { name: "Done" }));
    expect(screen.getByRole("button", { name: /Anthropic.*Connected/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Key rejected/ })).not.toBeInTheDocument();
  });

  it("focuses the catalogue search when it opens and the key field after picking a provider", async () => {
    renderPane();
    await userEvent.click(await screen.findByRole("button", { name: /Connect provider/ }));
    expect(document.activeElement).toBe(screen.getByLabelText("Search providers"));

    await userEvent.click(screen.getByRole("button", { name: /Google/ }));
    expect(document.activeElement).toBe(screen.getByLabelText("Google API key"));
  });
});
