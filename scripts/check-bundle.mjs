#!/usr/bin/env node
// Performance guard for the production webview bundle.
//
// Monaco is large and only needed when a diff or a spec is open. It must stay in a lazy chunk, never
// in the entry chunk the window parses at launch; otherwise a cold start pays for an editor nobody
// asked for. `vite build` cannot express that, so this runs after the build and fails loudly.
//
// Run it directly (`npm run check:bundle`, or as the last step of `npm run build`).

import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const dist = fileURLToPath(new URL("../apps/desktop/dist", import.meta.url));
const indexHtml = path.join(dist, "index.html");
const MARKER = "monaco-editor";

function fail(message) {
  console.error(`\nBundle guard failed: ${message}`);
  console.error(
    "Monaco must be reached through React.lazy/import() from the panel that needs it " +
      "(see architecture.md §Rendering and README §Performance notes).",
  );
  process.exit(1);
}

let html;
try {
  html = readFileSync(indexHtml, "utf8");
} catch (error) {
  fail(`cannot read ${indexHtml}: ${error.message}\nRun \`npm run build\` first.`);
}

// Vite writes the entry as <script type="module" src="/assets/index-<hash>.js">.
const entryMatch = html.match(/<script[^>]*\bsrc="([^"]+\.js)"/i);
if (!entryMatch) {
  fail("index.html has no module entry script; the build output is not what Vite writes");
}
const entryName = path.basename(entryMatch[1]);
const entryPath = path.join(dist, entryMatch[1].replace(/^\//, ""));
const entry = readFileSync(entryPath, "utf8");
const sizeKb = (text) => (Buffer.byteLength(text) / 1024).toFixed(1);

if (entry.toLowerCase().includes(MARKER)) {
  fail(
    `${entryName} (${sizeKb(entry)} kB) contains ${MARKER}: it is in the entry chunk, so every ` +
      "window launch parses the editor",
  );
}

const assetsDir = path.join(dist, "assets");
const chunks = readdirSync(assetsDir).filter((name) => name.endsWith(".js"));
const monacoChunks = [];
for (const name of chunks) {
  if (name === entryName) continue;
  const source = readFileSync(path.join(assetsDir, name), "utf8");
  if (source.toLowerCase().includes(MARKER)) {
    monacoChunks.push({ name, size: sizeKb(source) });
  }
}

if (monacoChunks.length === 0) {
  fail(
    `no lazy chunk contains ${MARKER}: either the editor was dropped from the panels or the ` +
      "imports stopped being dynamic",
  );
}

console.log(
  `Bundle guard: entry ${entryName} ${sizeKb(entry)} kB (no Monaco); ` +
    `lazy ${monacoChunks.map((chunk) => `${chunk.name} ${chunk.size} kB`).join(", ")}`,
);
