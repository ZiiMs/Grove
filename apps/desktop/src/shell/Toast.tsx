import { useEffect } from "react";
import { useUiStore } from "../state/uiStore";
import { Icon } from "../workspace/Icon";

/** One status line, bottom-centre. Replaces itself rather than stacking. */
export function Toast() {
  const toast = useUiStore((state) => state.toast);
  const clear = useUiStore((state) => state.clearToast);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(clear, 3200);
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
