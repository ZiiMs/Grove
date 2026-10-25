#!/usr/bin/env node
// Fast local Rust check: sccache cannot cache incremental crates, so the repo config forces
// CARGO_INCREMENTAL=0. That is right for clean builds, but it makes a one-file edit re-typecheck
// the whole crate. This command bypasses both for the edit loop; full tests and builds still use
// the repository's sccache setup.
import { spawnSync } from "node:child_process";

const args = process.argv.slice(2);
const hasPackage = args.includes("-p") || args.includes("--package");
const cargoArgs = [
  "check",
  ...(hasPackage ? args : ["-p", "grove-core", ...args]),
  "--config",
  'build.rustc-wrapper=""',
  "--config",
  'env.CARGO_INCREMENTAL.value="1"',
  "--config",
  "env.CARGO_INCREMENTAL.force=true",
];

const result = spawnSync(process.platform === "win32" ? "cargo.exe" : "cargo", cargoArgs, {
  stdio: "inherit",
});
if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}
process.exit(result.status ?? 1);
