import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const fromHere = (path: string) => fileURLToPath(new URL(path, import.meta.url));

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  resolve: {
    alias: {
      // Source alias so the contract package needs no build step (see architecture.md §IPC contract).
      "@grove/contract": fromHere("../../packages/contract/src/index.ts"),
    },
  },
  build: {
    target: "es2022",
    // Monaco and markdown-it are feature chunks; keep them out of the shell's entry chunk.
    chunkSizeWarningLimit: 900,
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    css: false,
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
