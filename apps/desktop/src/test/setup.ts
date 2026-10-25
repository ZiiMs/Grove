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
