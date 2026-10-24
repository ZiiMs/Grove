#!/usr/bin/env node
/**
 * The sidecar process: Grove Sidecar Protocol v1 on stdin/stdout, pi behind it.
 *
 * Nothing but protocol may go to stdout — diagnostics go to stderr, because the parent parses
 * stdout. Launched by the Rust `pi` adapter (default `node sidecar/dist/main.js`), or by anything
 * else that speaks the protocol:
 *
 *   node sidecar/dist/main.js [--pi-command <cmd>] [--pi-arg <arg>]...
 *
 * `GROVE_PI_COMMAND` is the environment equivalent of `--pi-command`. Per-session overrides arrive
 * in `session.start`'s `config.pi`, which is what the Grove settings and the tests use.
 */

import { attachSidecar } from "./sidecar.js";
import { createPiHost } from "./pi_host.js";
import type { PiLaunchConfig } from "./rpc.js";

function flag(argv: string[], name: string): string | undefined {
  const index = argv.indexOf(name);
  return index >= 0 && index + 1 < argv.length ? argv[index + 1] : undefined;
}

function repeated(argv: string[], name: string): string[] {
  const values: string[] = [];
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === name && index + 1 < argv.length) values.push(argv[index + 1]);
  }
  return values;
}

const argv = process.argv.slice(2);
const quiet = argv.includes("--quiet");
const args = repeated(argv, "--pi-arg");
const config: PiLaunchConfig = {
  command: flag(argv, "--pi-command") ?? process.env.GROVE_PI_COMMAND,
  args: args.length > 0 ? args : undefined,
};

const host = attachSidecar({ input: process.stdin, output: process.stdout }, (notify) =>
  createPiHost({
    notify,
    config,
    log: quiet ? () => {} : (message) => process.stderr.write(`grove-sidecar: ${message}\n`),
  }),
);

// The parent closing stdin is the shutdown signal: release every pi child, then exit. The signal
// handlers cover a terminal ^C and a polite `kill`; a hard kill is the parent's job (it kills the
// whole process tree, so pi never outlives the sidecar).
const shutdown = () => {
  void host.shutdown().finally(() => process.exit(0));
};
process.stdin.on("end", shutdown);
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

process.on("uncaughtException", (error) => {
  process.stderr.write(`grove-sidecar uncaught exception: ${error.stack ?? error.message}\n`);
  process.exit(1);
});
