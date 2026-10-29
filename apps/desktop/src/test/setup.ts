import "@testing-library/jest-dom/vitest";

/**
 * jsdom has no layout engine: every element reports a 0×0 box. Give elements a stable box and a
 * no-op `ResizeObserver` so components that measure their container behave like a first paint.
 */
if (!("ResizeObserver" in globalThis)) {
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  Object.defineProperty(globalThis, "ResizeObserver", {
    value: ResizeObserverStub,
    configurable: true,
    writable: true,
  });
}

for (const [name, value] of [
  ["clientHeight", 640],
  ["clientWidth", 800],
] as const) {
  Object.defineProperty(HTMLElement.prototype, name, {
    configurable: true,
    get: () => value,
  });
}

/**
 * jsdom has no PointerEvent, so `fireEvent.pointerDown` and friends would lose `clientX`/`button`.
 * MouseEvent carries both, so a thin subclass is enough for the resize handle's drag tests.
 */
if (!("PointerEvent" in window)) {
  class PointerEventStub extends MouseEvent {}
  Object.defineProperty(window, "PointerEvent", {
    configurable: true,
    writable: true,
    value: PointerEventStub,
  });
}

/** jsdom has no matchMedia; the shell uses it only to choose the mobile pane after an action. */
if (!("matchMedia" in window)) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}
