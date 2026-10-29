#!/usr/bin/env node
// Performance guard for the production webview bundle.
//
// The redesigned shell has no in-app code editor, so the original Monaco rule became a size budget:
// the entry chunk must stay small enough to parse at window launch, and Monaco must not quietly come
// back into it. Lazy feature chunks are still allowed for the parts that need them.
//
// Run it directly (`bun run check:bundle`, or as the last step of `bun run build`).

import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const dist = fileURLToPath(new URL("../apps/desktop/dist", import.meta.url));
const indexHtml = path.join(dist, "index.html");
const MARKER = "monaco-editor";
const ENTRY_BUDGET_KB = 600;

function fail(message) {
  console.error(`\nBundle guard failed: ${message}`);
  console.error(
    "Keep the entry chunk free of editors and hold it under the budget " +
      "(see architecture.md §Rendering and README §Performance notes).",
  );
  process.exit(1);
}

let html;
try {
  html = readFileSync(indexHtml, "utf8");
} catch (error) {
  fail(`cannot read ${indexHtml}: ${error.message}\nRun \`bun run build\` first.`);
}

// Vite writes the entry as <script type="module" src="/assets/index-<hash>.js">.
const entryMatch = html.match(/<script[^>]*\bsrc="([^"]+\.js)"/i);
if (!entryMatch) {
  fail("index.html has no module entry script; the build output is not what Vite writes");
}
const entryName = path.basename(entryMatch[1]);
const entryPath = path.join(dist, entryMatch[1].replace(/^\//, ""));
const entry = readFileSync(entryPath, "utf8");
const sizeKb = (text) => Buffer.byteLength(text) / 1024;

if (entry.toLowerCase().includes(MARKER)) {
  fail(`${entryName} contains ${MARKER}; an editor is back in the entry chunk`);
}
if (sizeKb(entry) > ENTRY_BUDGET_KB) {
  fail(
    `${entryName} is ${sizeKb(entry).toFixed(1)} kB, over the ${ENTRY_BUDGET_KB} kB entry budget`,
  );
}

const assetsDir = path.join(dist, "assets");
const chunks = readdirSync(assetsDir).filter((name) => name.endsWith(".js"));
const heavy = chunks
  .filter((name) => name !== entryName)
  .map((name) => ({ name, kb: sizeKb(readFileSync(path.join(assetsDir, name), "utf8")) }))
  .sort((left, right) => right.kb - left.kb)
  .slice(0, 3);

console.log(
  `Bundle guard: entry ${entryName} ${sizeKb(entry).toFixed(1)} kB (budget ${ENTRY_BUDGET_KB} kB); ` +
    `lazy ${heavy.map((chunk) => `${chunk.name} ${chunk.kb.toFixed(1)} kB`).join(", ") || "none"}`,
);
