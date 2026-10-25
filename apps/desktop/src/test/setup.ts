import "@testing-library/jest-dom/vitest";

/**
 * jsdom has no layout engine: every element reports a 0×0 box. Virtualized lists therefore render no
 * rows, which would make the Sessions conversation untestable. Give elements a stable box and a no-op
 * `ResizeObserver` (jsdom has none) so `@tanstack/react-virtual` renders the first paint, which is the
 * behaviour the browser shows too.
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
