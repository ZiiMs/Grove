import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const fromHere = (path: string) => fileURLToPath(new URL(path, import.meta.url));

/**
 * The visual comparison harness. It is not part of `bun run test`: it renders the built shell into
 * `dist/visual-*.html` so a headless browser can screenshot it at the contract's viewports.
 * Run with `bun run visual` from `apps/desktop` (which builds first).
 */
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@grove/contract": fromHere("../../packages/contract/src/index.ts"),
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    css: false,
    include: ["tools/**/*.test.{ts,tsx}"],
  },
});
