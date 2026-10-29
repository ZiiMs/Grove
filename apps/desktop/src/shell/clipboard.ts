/**
 * Clipboard access for the shell and the context menu.
 *
 * A non-component module: panels and the menu host both copy text, and neither should have to import
 * the menu component to do it. The async Clipboard API goes first; the hidden-textarea legacy path
 * keeps copy working in a webview that denies it.
 */
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
