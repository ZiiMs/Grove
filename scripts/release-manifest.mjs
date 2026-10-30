#!/usr/bin/env node
/**
 * Turn a signed Tauri NSIS build into the files a GitHub release needs:
 *
 *   latest.json   the Tauri updater manifest (`windows-x86_64` entry)
 *   SHA256SUMS    integrity checks for the installer
 *
 * On Windows the updater artifact is the NSIS installer itself plus its `.sig`, so the manifest
 * points at `*-setup.exe`. The updater trusts that minisign signature, not the checksum; the
 * checksum is for people downloading the installer by hand.
 *
 *   bun scripts/release-manifest.mjs --version 0.3.1 --tag v0.3.1 --repo ZiiMs/Grove \
 *     --notes .release/notes.md --bundle-dir target/release/bundle/nsis
 */
import { createHash } from "node:crypto";
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

function argument(argv, name, fallback) {
  const index = argv.indexOf(`--${name}`);
  if (index < 0) {
    if (fallback !== undefined) return fallback;
    throw new Error(`Missing --${name}`);
  }
  return argv[index + 1];
}

function single(dir, suffix) {
  const matches = readdirSync(dir).filter(
    (name) => name.endsWith(suffix) && !name.endsWith(".sig"),
  );
  if (matches.length !== 1) {
    throw new Error(`Expected one *${suffix} in ${dir}, found ${matches.length}`);
  }
  return matches[0];
}

export function manifest({ version, notes, signature, url, date = new Date() }) {
  return {
    version,
    notes,
    pub_date: date.toISOString(),
    platforms: {
      "windows-x86_64": { signature, url },
    },
  };
}

export function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

function main(argv) {
  const version = argument(argv, "version");
  const tag = argument(argv, "tag");
  const repo = argument(argv, "repo");
  const notesFile = argument(argv, "notes", ".release/notes.md");
  const bundleDir = resolve(argument(argv, "bundle-dir", "target/release/bundle/nsis"));
  const outDir = resolve(argument(argv, "out-dir", ".release"));

  if (!existsSync(bundleDir)) throw new Error(`Bundle directory not found: ${bundleDir}`);
  const installer = single(bundleDir, "-setup.exe");
  const installerPath = join(bundleDir, installer);
  const sigPath = `${installerPath}.sig`;
  if (!existsSync(sigPath)) {
    throw new Error(
      `Missing ${sigPath}. Build with bundle.createUpdaterArtifacts=true and the signing key set.`,
    );
  }

  const signature = readFileSync(sigPath, "utf8").trim();
  const decoded = Buffer.from(signature, "base64").toString("utf8");
  if (!decoded.startsWith("untrusted comment:")) {
    throw new Error(`${sigPath} is not a base64 minisign signature.`);
  }

  const notes = readFileSync(notesFile, "utf8").trim();
  const url = `https://github.com/${repo}/releases/download/${tag}/${installer}`;
  const body = manifest({ version, notes, signature, url });

  mkdirSync(outDir, { recursive: true });
  const latestPath = join(outDir, "latest.json");
  writeFileSync(latestPath, `${JSON.stringify(body, null, 2)}\n`);

  const sumsPath = join(outDir, "SHA256SUMS");
  writeFileSync(sumsPath, `${sha256(installerPath)}  ${installer}\n`);

  const toPosix = (path) => path.replaceAll("\\", "/");
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(
      process.env.GITHUB_OUTPUT,
      [
        `installer=${toPosix(installerPath)}`,
        `sig=${toPosix(sigPath)}`,
        `latest=${toPosix(latestPath)}`,
        `sums=${toPosix(sumsPath)}`,
        "",
      ].join("\n"),
    );
  }
  console.log(`Wrote ${latestPath} and ${sumsPath} for ${tag}.`);
}

const invokedDirectly =
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url;
if (invokedDirectly) main(process.argv.slice(2));
