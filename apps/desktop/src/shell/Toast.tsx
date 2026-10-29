import { useEffect } from "react";
import { useUiStore } from "../state/uiStore";
import { Icon } from "../workspace/Icon";

/** One status line, bottom-right. Replaces itself rather than stacking. */
export function Toast() {
  const toast = useUiStore((state) => state.toast);
  const clear = useUiStore((state) => state.clearToast);

  useEffect(() => {
    if (!toast) return;
    // Long enough for a failure message (such as a title-model error naming the provider) to be
    // read before the line disappears.
    const timer = window.setTimeout(clear, 4000);
    return () => window.clearTimeout(timer);
  }, [clear, toast]);

  return (
    <div className="toast" role="status" aria-live="polite" data-open={toast ? "true" : "false"}>
      <span className="t-ico" aria-hidden="true">
        <Icon name={toast?.tone === "error" ? "alert" : "checkCircle"} size="sm" />
      </span>
      <span>{toast?.message ?? ""}</span>
    </div>
  );
}
