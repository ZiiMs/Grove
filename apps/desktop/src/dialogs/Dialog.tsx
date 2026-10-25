import { useEffect, useRef, type ReactNode } from "react";

/**
 * A native dialog that stays unmounted until open. `showModal()` gives the focus trap and the
 * `::backdrop` the design draws; environments without it (jsdom) fall back to the `open` attribute.
 */
export function Dialog({
  open,
  className,
  labelledBy,
  onClose,
  children,
}: {
  open: boolean;
  className?: string;
  labelledBy?: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    if (open && !node.open) {
      if (typeof node.showModal === "function") {
        try {
          node.showModal();
        } catch {
          node.setAttribute("open", "");
        }
      } else {
        node.setAttribute("open", "");
      }
    } else if (!open && node.open) {
      if (typeof node.close === "function") node.close();
      else node.removeAttribute("open");
    }
  }, [open]);

  if (!open) return null;

  return (
    <dialog
      ref={ref}
      className={className}
      aria-labelledby={labelledBy}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === ref.current) onClose();
      }}
    >
      {children}
    </dialog>
  );
}
