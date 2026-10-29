import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import type { GroveBridge } from "../ipc/bridge";
import { useBridge } from "../ipc/context";
import { errorText } from "../ipc/errors";
import { useMenuStore, type MenuItem, type MenuState } from "../state/menuStore";
import { useSessionStore } from "../state/sessionStore";
import { useUiStore } from "../state/uiStore";
import { useWorkspaceStore } from "../state/workspaceStore";
import { Icon } from "../workspace/Icon";

/**
 * The one context menu in the app.
 *
 * Components put a menu in `menuStore`; this host draws it in the topmost layer, keeps it inside the
 * viewport and gives it the keyboard behaviour the platform menu would have had. A native `<dialog>`
 * lives in the browser's top layer, so the menu is portalled into the topmost open dialog when there
 * is one; a body portal would render underneath it.
 */

const MARGIN = 8;

function portalTarget(): HTMLElement {
  const dialogs = document.querySelectorAll<HTMLDialogElement>("dialog[open]");
  return dialogs.length > 0 ? dialogs[dialogs.length - 1] : document.body;
}

/** Copy with the async Clipboard API, falling back to the hidden-textarea trick. */
export async function copyText(text: string): Promise<boolean> {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // Permission denied: try the legacy path before reporting failure.
    }
  }
  if (typeof document.execCommand !== "function") return false;
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  try {
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    area.remove();
  }
}

/** Show a path in the platform file manager; a failure is reported, never thrown at the caller. */
export async function revealPath(
  bridge: GroveBridge,
  path: string,
  notify: (message: string, tone?: "ok" | "error") => void,
): Promise<void> {
  try {
    await bridge.invoke("app_reveal_path", { path });
  } catch (error) {
    notify(errorText(error), "error");
  }
}

function menuButtons(root: HTMLElement | null): HTMLButtonElement[] {
  if (!root) return [];
  return Array.from(
    root.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]:not(:disabled)'),
  );
}

function isTextField(target: Element | null): HTMLElement | null {
  if (!target) return null;
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return target;
  const editable = target.closest('[contenteditable]:not([contenteditable="false"])');
  return editable instanceof HTMLElement ? editable : null;
}

function canExec(): boolean {
  return typeof document.execCommand === "function";
}

/** Run a legacy editing command; false means the caller should use its fallback. */
function runCommand(command: string): boolean {
  if (!canExec()) return false;
  try {
    return document.execCommand(command);
  } catch {
    return false;
  }
}

function isField(field: HTMLElement): field is HTMLInputElement | HTMLTextAreaElement {
  return field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement;
}

function selectedText(field: HTMLElement): string {
  if (isField(field)) {
    const start = field.selectionStart ?? 0;
    const end = field.selectionEnd ?? start;
    return field.value.slice(start, end);
  }
  return window.getSelection()?.toString() ?? "";
}

function notifyInput(field: HTMLElement) {
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

function replaceSelection(field: HTMLElement, text: string) {
  if (isField(field)) {
    const start = field.selectionStart ?? field.value.length;
    const end = field.selectionEnd ?? start;
    field.setRangeText(text, start, end, "end");
    notifyInput(field);
    return;
  }
  const selection = window.getSelection();
  if (selection && selection.rangeCount > 0 && field.contains(selection.anchorNode)) {
    const range = selection.getRangeAt(0);
    range.deleteContents();
    const node = document.createTextNode(text);
    range.insertNode(node);
    range.setStartAfter(node);
    range.collapse(true);
    selection.removeAllRanges();
    selection.addRange(range);
  } else {
    field.appendChild(document.createTextNode(text));
  }
  notifyInput(field);
}

function selectFieldContents(field: HTMLElement) {
  if (isField(field)) {
    field.select();
    return;
  }
  const range = document.createRange();
  range.selectNodeContents(field);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
}

const isMac = typeof navigator !== "undefined" && /mac/i.test(navigator.platform ?? "");
const shortcut = (key: string) => (isMac ? `⌘${key}` : `Ctrl+${key}`);

/**
 * The native text-field menu. `execCommand` goes first because it preserves the field's own undo
 * stack; copy, cut and paste fall back to the Clipboard API when it is missing. `clipboardText` is
 * `null` when the read was denied or unavailable, which is the one case Paste is omitted instead of
 * being shown as a dead item.
 */
function editMenuItems(field: HTMLElement, clipboardText: string | null): MenuItem[] {
  const exec = canExec();
  const copy = () => {
    field.focus();
    if (runCommand("copy")) return;
    const text = selectedText(field);
    if (text) void copyText(text);
  };
  const items: MenuItem[] = [
    {
      kind: "item",
      label: "Undo",
      shortcut: shortcut("Z"),
      disabled: !exec,
      onSelect: () => {
        field.focus();
        runCommand("undo");
      },
    },
    {
      kind: "item",
      label: "Redo",
      shortcut: shortcut("Y"),
      disabled: !exec,
      onSelect: () => {
        field.focus();
        runCommand("redo");
      },
    },
    { kind: "separator" },
    {
      kind: "item",
      label: "Cut",
      shortcut: shortcut("X"),
      onSelect: () => {
        field.focus();
        if (runCommand("cut")) return;
        const text = selectedText(field);
        if (!text) return;
        void copyText(text).then((copied) => {
          if (copied) replaceSelection(field, "");
        });
      },
    },
    { kind: "item", label: "Copy", shortcut: shortcut("C"), onSelect: copy },
  ];
  if (clipboardText !== null) {
    items.push({
      kind: "item",
      label: "Paste",
      shortcut: shortcut("V"),
      onSelect: () => {
        field.focus();
        if (!runCommand("paste")) replaceSelection(field, clipboardText);
      },
    });
  }
  items.push({
    kind: "item",
    label: "Select all",
    shortcut: shortcut("A"),
    onSelect: () => {
      field.focus();
      if (!runCommand("selectAll")) selectFieldContents(field);
    },
  });
  return items;
}

async function openEditMenu(field: HTMLElement, x: number, y: number) {
  let clipboardText: string | null = null;
  if (typeof navigator !== "undefined" && navigator.clipboard?.readText) {
    try {
      clipboardText = await navigator.clipboard.readText();
    } catch {
      clipboardText = null;
    }
  }
  if (!field.isConnected) return;
  useMenuStore.getState().openMenu({
    x,
    y,
    returnFocus: field,
    items: editMenuItems(field, clipboardText),
  });
}

function MenuSurface({ state }: { state: MenuState }) {
  const closeMenu = useMenuStore((store) => store.closeMenu);
  const ref = useRef<HTMLDivElement>(null);
  const [left, setLeft] = useState(state.x);
  const [top, setTop] = useState(state.y);

  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const box = node.getBoundingClientRect();
    const width = box.width;
    const height = box.height;
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;

    // Open toward the pointer, but flip to the other side when that would overflow, then clamp so a
    // menu near an edge is never cut off.
    let x = state.x;
    let y = state.y;
    if (x + width + MARGIN > viewportWidth) x -= width;
    if (y + height + MARGIN > viewportHeight) y -= height;
    x = Math.max(MARGIN, Math.min(x, viewportWidth - width - MARGIN));
    y = Math.max(MARGIN, Math.min(y, viewportHeight - height - MARGIN));

    setLeft(x);
    setTop(y);
    menuButtons(node)[0]?.focus();
  }, [state]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const buttons = menuButtons(ref.current);
    if (buttons.length === 0 && event.key !== "Escape" && event.key !== "Tab") return;
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);

    if (event.key === "ArrowDown") {
      event.preventDefault();
      buttons[(index + 1) % buttons.length]?.focus();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      buttons[(index - 1 + buttons.length) % buttons.length]?.focus();
    } else if (event.key === "Home") {
      event.preventDefault();
      buttons[0]?.focus();
    } else if (event.key === "End") {
      event.preventDefault();
      buttons[buttons.length - 1]?.focus();
    } else if (event.key === "Enter" || event.key === " ") {
      const active = document.activeElement;
      if (active instanceof HTMLButtonElement && buttons.includes(active)) {
        // Handle activation here so a key press cannot both click and be handled twice.
        event.preventDefault();
        active.click();
      }
    } else if (event.key === "Escape" || event.key === "Tab") {
      event.preventDefault();
      closeMenu();
    }
  };

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (ref.current && event.target instanceof Node && ref.current.contains(event.target)) return;
      closeMenu();
    };
    const close = () => closeMenu();
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, [closeMenu]);

  return (
    <div
      className="ctx-menu"
      data-grove-menu=""
      role="menu"
      aria-label="Context menu"
      ref={ref}
      style={{ left, top }}
      onKeyDown={onKeyDown}
      onContextMenu={(event) => event.preventDefault()}
    >
      {state.items.map((item, index) =>
        item.kind === "separator" ? (
          <div className="ctx-sep" role="separator" key={`sep-${index}`} />
        ) : (
          <button
            className="ctx-item"
            type="button"
            role="menuitem"
            key={`${item.label}-${index}`}
            data-danger={item.danger ? "true" : undefined}
            disabled={item.disabled}
            tabIndex={-1}
            onClick={() => {
              closeMenu();
              item.onSelect();
            }}
          >
            <span className="ctx-ico">{item.icon ? <Icon name={item.icon} size="sm" /> : null}</span>
            <span className="ctx-label">{item.label}</span>
            {item.shortcut ? (
              <span className="ctx-shortcut" aria-hidden="true">
                {item.shortcut}
              </span>
            ) : null}
          </button>
        ),
      )}
    </div>
  );
}

/** Mount once, in the shell. Owns the native-menu suppression and the background menu. */
export function ContextMenuHost() {
  const bridge = useBridge();
  const menu = useMenuStore((store) => store.menu);
  const closeMenu = useMenuStore((store) => store.closeMenu);
  const previous = useRef<MenuState | null>(null);

  // Return focus only when the menu actually closes; replacing one menu with another keeps focus.
  useEffect(() => {
    const closed = previous.current;
    previous.current = menu;
    if (!closed || menu) return;
    const target = closed.returnFocus;
    if (!target || !target.isConnected) return;
    // An action may have opened a dialog above the invoking surface; that dialog owns focus.
    const topmost = document.querySelector("dialog[open]");
    if (topmost && !topmost.contains(target)) return;
    target.focus();
  }, [menu]);

  useEffect(() => {
    const onContextMenu = (event: MouseEvent) => {
      // A component already opened its own menu (and suppressed the native one): leave it alone.
      if (event.defaultPrevented) return;
      event.preventDefault();
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest("[data-grove-menu]")) return;

      const field = isTextField(target);
      if (field) {
        void openEditMenu(field, event.clientX, event.clientY);
        return;
      }

      const openNewSession = useUiStore.getState().openNewSession;
      const openAddProject = useUiStore.getState().openAddProject;
      useMenuStore.getState().openMenu({
        x: event.clientX,
        y: event.clientY,
        returnFocus: target instanceof HTMLElement ? target : null,
        items: [
          {
            kind: "item",
            label: "New chat",
            icon: "message",
            onSelect: () => openNewSession({ projectId: null, worktreePath: null }),
          },
          { kind: "item", label: "Add project", icon: "folderPlus", onSelect: openAddProject },
          { kind: "separator" },
          {
            kind: "item",
            label: "Refresh",
            icon: "refresh",
            onSelect: () => {
              void useWorkspaceStore.getState().load(bridge);
              void useSessionStore.getState().load(bridge);
            },
          },
        ],
      });
    };
    document.addEventListener("contextmenu", onContextMenu);
    return () => document.removeEventListener("contextmenu", onContextMenu);
  }, [bridge]);

  useEffect(() => () => closeMenu(), [closeMenu]);

  if (!menu) return null;
  return createPortal(<MenuSurface state={menu} />, portalTarget());
}
