import { useCallback, useEffect, useRef, useState } from "react";

export const NAV_MIN = 220;
export const NAV_MAX = 520;
export const NAV_DEFAULT = 300;
const STORAGE_KEY = "grove.navWidth";

function clamp(value: number): number {
  return Math.min(NAV_MAX, Math.max(NAV_MIN, Math.round(value)));
}

function storedWidth(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const value = raw ? Number(raw) : Number.NaN;
    return Number.isFinite(value) ? clamp(value) : NAV_DEFAULT;
  } catch {
    return NAV_DEFAULT;
  }
}

/** The dragged sidebar width, remembered across launches; `null` restores the default. */
export function useNavWidth(): [number, (width: number | null) => void] {
  const [width, setWidth] = useState(storedWidth);
  const set = useCallback((next: number | null) => {
    if (next === null) {
      try {
        localStorage.removeItem(STORAGE_KEY);
      } catch {
        // Storage can be unavailable; the width still changes for this run.
      }
      setWidth(NAV_DEFAULT);
      return;
    }
    const clamped = clamp(next);
    try {
      localStorage.setItem(STORAGE_KEY, String(clamped));
    } catch {
      // Same: an unwritable store must not block resizing.
    }
    setWidth(clamped);
  }, []);
  return [width, set];
}

/**
 * The drag handle on the sidebar's right edge. Pointer moves are tracked on the window so the drag
 * survives leaving the 8px strip; double-click restores the default, and the arrow keys resize from
 * the keyboard.
 */
export function NavResizer({
  width,
  onChange,
}: {
  width: number;
  onChange: (width: number | null) => void;
}) {
  const [dragging, setDragging] = useState(false);
  const origin = useRef({ x: 0, width });

  useEffect(() => {
    if (!dragging) return;
    const move = (event: PointerEvent) => {
      onChange(origin.current.width + event.clientX - origin.current.x);
    };
    const stop = () => setDragging(false);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
    window.addEventListener("pointercancel", stop, { once: true });
    document.body.classList.add("nav-resizing");
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      document.body.classList.remove("nav-resizing");
    };
  }, [dragging, onChange]);

  return (
    <div
      className="nav-resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize the sidebar"
      aria-valuemin={NAV_MIN}
      aria-valuemax={NAV_MAX}
      aria-valuenow={width}
      tabIndex={0}
      data-dragging={dragging ? "true" : undefined}
      title="Drag to resize · double-click to reset"
      onPointerDown={(event) => {
        // `button` is absent on the jsdom MouseEvent shim used by tests; only a secondary real
        // button should be ignored.
        if (event.button > 0) return;
        event.preventDefault();
        origin.current = { x: event.clientX, width };
        setDragging(true);
      }}
      onDoubleClick={() => onChange(null)}
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft") {
          event.preventDefault();
          onChange(width - 16);
        } else if (event.key === "ArrowRight") {
          event.preventDefault();
          onChange(width + 16);
        } else if (event.key === "Home") {
          event.preventDefault();
          onChange(NAV_MIN);
        } else if (event.key === "End") {
          event.preventDefault();
          onChange(NAV_DEFAULT);
        }
      }}
    />
  );
}
