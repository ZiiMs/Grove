/**
 * PROTOTYPE — throwaway. Floating variant switcher: ← → cycles, URL stays shareable.
 * Hidden from production by PrototypeApp's own gate (`?prototype`).
 */

import { useEffect } from "react";

interface Props {
  label: string;
  hint?: string;
  onPrev: () => void;
  onNext: () => void;
}

function isTyping(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT" ||
    target.isContentEditable
  );
}

export function PrototypeSwitcher({ label, hint, onPrev, onNext }: Props) {
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (isTyping(event.target)) return;
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        onPrev();
      }
      if (event.key === "ArrowRight") {
        event.preventDefault();
        onNext();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onPrev, onNext]);

  return (
    <div className="pv-switcher" data-testid="prototype-switcher">
      <button type="button" onClick={onPrev} aria-label="Previous">
        ←
      </button>
      <span className="pv-switcher-label">
        <b>{label}</b>
        {hint ? ` · ${hint}` : ""}
      </span>
      <button type="button" onClick={onNext} aria-label="Next">
        →
      </button>
    </div>
  );
}
