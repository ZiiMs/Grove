#!/usr/bin/env node
/**
 * Grove's release planner.
 *
 * The git tag is the source of truth for the released version. This script reads conventional
 * commits since the last reachable `v*` tag, works out the next SemVer version, keeps the five
 * version surfaces in step, and writes the changelog.
 *
 *   bun scripts/release.mjs --check     verify every version surface agrees
 *   bun scripts/release.mjs --dry-run   print the next version and notes, change nothing
 *   bun scripts/release.mjs --apply     write the version files and CHANGELOG.md
 *   bun scripts/release.mjs --commit    apply, commit, tag and push (CI only)
 *
 * `--commit` serialises on main: it re-reads origin/main on every attempt and retries the atomic
 * push if a human merged something while the release was being prepared.
 */
import { spawnSync } from "node:child_process";
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const CHANGELOG = join(ROOT, "CHANGELOG.md");
const RELEASE_DIR = join(ROOT, ".release");
const VERSION_FILES = [
  "package.json",
  "apps/desktop/package.json",
  "packages/contract/package.json",
  "package-lock.json",
  "Cargo.toml",
  "apps/desktop/src-tauri/tauri.conf.json",
];
const KNOWN_TYPES = new Set([
  "feat",
  "fix",
  "perf",
  "refactor",
  "style",
  "revert",
  "docs",
  "build",
  "ci",
  "chore",
  "test",
]);

// ---------------------------------------------------------------------------
// Pure helpers (covered by scripts/release.test.mjs)
// ---------------------------------------------------------------------------

/** Return the `version` under `[workspace.package]`, or null. */
export function cargoWorkspaceVersion(text) {
  const section = text.match(/^\[workspace\.package\][^[]*/m);
  if (!section) return null;
  return section[0].match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? null;
}

/** Return a workspace crate's version from Cargo.lock, or null. */
export function cargoLockVersion(text, name) {
  const pattern = new RegExp(
    `\\[\\[package\\]\\]\\nname = "${name}"\\nversion = "([^"]+)"`,
  );
  return text.match(pattern)?.[1] ?? null;
}

/**
 * Parse one conventional commit. Returns null for merge and release commits, which are noise in a
 * changelog. Unknown types are kept under `other` rather than dropped.
 */
export function parseCommit(subject, body = "") {
  if (/^chore\(release\):/i.test(subject)) return null;
  if (/^chore:\s*release\b/i.test(subject)) return null;
  if (/^Merge (pull request|branch|remote-tracking)/i.test(subject)) return null;

  const match = subject.match(/^([A-Za-z]+)(?:\(([^)]+)\))?(!)?:\s*(.+)$/);
  const breaking = Boolean(match?.[3]) || /BREAKING[ -]CHANGE:/.test(body);
  if (!match) {
    return { type: "other", scope: null, description: subject.trim(), breaking };
  }
  const type = match[1].toLowerCase();
  return {
    type: KNOWN_TYPES.has(type) ? type : "other",
    scope: match[2] ?? null,
    description: match[4].trim(),
    breaking,
  };
}

/**
 * Pre-1.0, a breaking change and a feature both move the minor version; fixes move the patch.
 * From 1.0.0, breaking moves the major.
 */
export function bumpVersion(version, commits) {
  const parts = version.split(".").map(Number);
  if (parts.length !== 3 || parts.some((part) => !Number.isInteger(part) || part < 0)) {
    throw new Error(`Not a SemVer version: ${version}`);
  }
  const [major, minor, patch] = parts;
  const breaking = commits.some((commit) => commit.breaking);
  const feature = commits.some((commit) => commit.type === "feat");

  if (major === 0) {
    if (breaking || feature) return `0.${minor + 1}.0`;
    return `0.${minor}.${patch + 1}`;
  }
  if (breaking) return `${major + 1}.0.0`;
  if (feature) return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

function commitLine(commit) {
  return commit.scope
    ? `- **${commit.scope}:** ${commit.description}`
    : `- ${commit.description}`;
}

function section(title, commits) {
  const lines = [...new Set(commits.map(commitLine))];
  return `### ${title}\n\n${lines.join("\n")}`;
}

/** Release notes for a set of commits, grouped the way the old releases were. */
export function renderNotes(commits) {
  const sections = [];
  const breaking = commits.filter((commit) => commit.breaking);
  if (breaking.length) sections.push(section("Breaking changes", breaking));

  const groups = [
    ["Added", ["feat"]],
    ["Changed", ["perf", "refactor", "style", "revert"]],
    ["Fixed", ["fix"]],
    ["Docs", ["docs"]],
    ["Other", ["build", "ci", "chore", "test", "other"]],
  ];
  for (const [title, types] of groups) {
    const rows = commits.filter(
      (commit) => !commit.breaking && types.includes(commit.type),
    );
    if (rows.length) sections.push(section(title, rows));
  }
  return sections.join("\n\n") || "No user-facing changes.";
}

/** The block appended to CHANGELOG.md for one version. */
export function changelogSection(version, notes, date = new Date()) {
  return `## v${version} — ${date.toISOString().slice(0, 10)}\n\n${notes}`;
}

/**
 * Insert `section` after the CHANGELOG intro. Re-running for the same version replaces that
 * version's block instead of duplicating it, which keeps workflow re-runs idempotent.
 */
export function upsertChangelog(existing, section) {
  const header =
    "# Changelog\n\nAll notable changes to Grove are recorded here. Versions follow SemVer and\n" +
    "entries are generated from conventional commits by `scripts/release.mjs`.\n";
  if (!existing.trim()) return `${header}\n${section}\n`;

  const marker = existing.indexOf("\n## v");
  if (marker < 0) return `${existing.trimEnd()}\n\n${section}\n`;

  const head = existing.slice(0, marker + 1);
  const rest = existing.slice(marker + 1);
  const next = rest.indexOf("\n## v");
  const current = next < 0 ? rest : rest.slice(0, next + 1);
  const tail = next < 0 ? "" : rest.slice(next + 1);
  const version = section.match(/^## (v[^\s]+)/)?.[1];

  if (version && current.startsWith(`## ${version} `)) {
    return `${head}${section}\n\n${tail}`.trimEnd() + "\n";
  }
  return `${head}${section}\n\n${rest}`.trimEnd() + "\n";
}

// ---------------------------------------------------------------------------
// Git and filesystem
// ---------------------------------------------------------------------------

function run(command, args, { cwd = ROOT, allowFail = false } = {}) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8" });
  const ok = result.status === 0;
  if (!ok && !allowFail) {
    throw new Error(
      `${command} ${args.join(" ")} failed (${result.status ?? "no status"}): ` +
        `${(result.stderr || result.stdout || "").trim()}`,
    );
  }
  return {
    ok,
    stdout: (result.stdout ?? "").trim(),
    stderr: (result.stderr ?? "").trim(),
    status: result.status,
  };
}

function git(args, options) {
  return run("git", args, options);
}

function readJson(relative) {
  return JSON.parse(readFileSync(join(ROOT, relative), "utf8"));
}

function writeJson(relative, value) {
  writeFileSync(join(ROOT, relative), `${JSON.stringify(value, null, 2)}\n`);
}

const SEMVER_TAG = /^v\d+\.\d+\.\d+$/;

function lastTag() {
  // Highest SemVer tag reachable from HEAD. Release tags are monotonic, and `--merged` keeps the
  // old pre-rewrite tags out because they are not ancestors of this history.
  const result = git(
    ["tag", "--merged", "HEAD", "--list", "v*", "--sort=-v:refname"],
    { allowFail: true },
  );
  return (
    result.stdout
      .split("\n")
      .map((tag) => tag.trim())
      .find((tag) => SEMVER_TAG.test(tag)) ?? null
  );
}

function exactTag() {
  const result = git(
    ["tag", "--points-at", "HEAD", "--list", "v*", "--sort=-v:refname"],
    { allowFail: true },
  );
  return (
    result.stdout
      .split("\n")
      .map((tag) => tag.trim())
      .find((tag) => SEMVER_TAG.test(tag)) ?? null
  );
}

function commitsSince(tag) {
  const range = tag ? `${tag}..HEAD` : "HEAD";
  const result = git([
    "log",
    range,
    "--no-merges",
    "--format=%s%x1f%b%x1e",
  ]);
  return result.stdout
    .split("\x1e")
    .map((record) => record.trim())
    .filter(Boolean)
    .map((record) => {
      const [subject, ...body] = record.split("\x1f");
      return parseCommit(subject, body.join("\x1f"));
    })
    .filter(Boolean);
}

/** All version surfaces, keyed by the file (and lock section) that carries them. */
export function readVersions() {
  const lock = readJson("package-lock.json");
  const cargo = readFileSync(join(ROOT, "Cargo.toml"), "utf8");
  const cargoLock = readFileSync(join(ROOT, "Cargo.lock"), "utf8");
  return new Map([
    ["package.json", readJson("package.json").version],
    ["apps/desktop/package.json", readJson("apps/desktop/package.json").version],
    ["packages/contract/package.json", readJson("packages/contract/package.json").version],
    ["Cargo.toml", cargoWorkspaceVersion(cargo)],
    [
      "apps/desktop/src-tauri/tauri.conf.json",
      readJson("apps/desktop/src-tauri/tauri.conf.json").version,
    ],
    ["package-lock.json", lock.version],
    ["package-lock.json (workspace root)", lock.packages?.[""]?.version],
    ["package-lock.json (apps/desktop)", lock.packages?.["apps/desktop"]?.version],
    ["package-lock.json (packages/contract)", lock.packages?.["packages/contract"]?.version],
    ["Cargo.lock (grove-core)", cargoLockVersion(cargoLock, "grove-core")],
    ["Cargo.lock (grove-desktop)", cargoLockVersion(cargoLock, "grove-desktop")],
  ]);
}

export function checkVersions() {
  const versions = readVersions();
  const expected = versions.get("package.json");
  const mismatches = [...versions].filter(([, version]) => version !== expected);
  if (mismatches.length) {
    const detail = mismatches
      .map(([file, version]) => `  ${file}: ${version ?? "(missing)"} != ${expected}`)
      .join("\n");
    throw new Error(`Version surfaces disagree:\n${detail}`);
  }
  return expected;
}

export function writeVersions(version) {
  for (const file of [
    "package.json",
    "apps/desktop/package.json",
    "packages/contract/package.json",
  ]) {
    const json = readJson(file);
    json.version = version;
    writeJson(file, json);
  }

  const tauri = readJson("apps/desktop/src-tauri/tauri.conf.json");
  tauri.version = version;
  writeJson("apps/desktop/src-tauri/tauri.conf.json", tauri);

  const lock = readJson("package-lock.json");
  lock.version = version;
  if (lock.packages?.[""]) lock.packages[""].version = version;
  if (lock.packages?.["apps/desktop"]) lock.packages["apps/desktop"].version = version;
  if (lock.packages?.["packages/contract"]) {
    lock.packages["packages/contract"].version = version;
  }
  writeJson("package-lock.json", lock);

  const cargoPath = join(ROOT, "Cargo.toml");
  const cargo = readFileSync(cargoPath, "utf8");
  const current = cargoWorkspaceVersion(cargo);
  if (!current) throw new Error("Cargo.toml: [workspace.package] version not found");
  if (current !== version) {
    const next = cargo.replace(
      /(^\[workspace\.package\][^[]*?^version\s*=\s*")[^"]+(")/m,
      `$1${version}$2`,
    );
    if (next === cargo) throw new Error("Cargo.toml: [workspace.package] version not found");
    writeFileSync(cargoPath, next);
  }

  const cargoLockPath = join(ROOT, "Cargo.lock");
  let cargoLock = readFileSync(cargoLockPath, "utf8");
  for (const name of ["grove-core", "grove-desktop"]) {
    const pattern = new RegExp(
      `(\\[\\[package\\]\\]\\nname = "${name}"\\nversion = ")[^"]+(")`,
    );
    if (!pattern.test(cargoLock)) {
      throw new Error(`Cargo.lock: package ${name} not found`);
    }
    cargoLock = cargoLock.replace(pattern, `$1${version}$2`);
  }
  writeFileSync(cargoLockPath, cargoLock);
}

/** Work out what this push should release. */
export function planRelease() {
  const packageVersion = readJson("package.json").version;
  const tag = lastTag();
  const commits = commitsSince(tag);

  if (tag && commits.length === 0) {
    const head = exactTag();
    if (head) {
      return {
        skip: true,
        version: head.replace(/^v/, ""),
        tag: head,
        previousTag: tag,
        commits: [],
      };
    }
    throw new Error(
      `No releasable commits since ${tag}, but HEAD is not tagged. ` +
        "A previous release probably pushed its commit but not its tag; tag HEAD or revert it.",
    );
  }

  const base = tag ? tag.replace(/^v/, "") : packageVersion;
  const version = tag ? bumpVersion(base, commits) : base;
  return {
    skip: false,
    version,
    tag: `v${version}`,
    previousTag: tag,
    commits,
    notes: renderNotes(commits),
  };
}

function readChangelogSection(version) {
  if (!existsSync(CHANGELOG)) return "No notes recorded.";
  const text = readFileSync(CHANGELOG, "utf8");
  const start = text.indexOf(`## v${version} `);
  if (start < 0) return "No notes recorded.";
  const end = text.indexOf("\n## v", start + 1);
  return text
    .slice(start, end < 0 ? undefined : end)
    .replace(/^## v[^\n]+\n+/, "")
    .trim();
}

function writeReleaseFiles(plan, notes) {
  mkdirSync(RELEASE_DIR, { recursive: true });
  writeFileSync(join(RELEASE_DIR, "notes.md"), `${notes}\n`);
  const { commits, ...rest } = plan;
  writeFileSync(
    join(RELEASE_DIR, "release.json"),
    `${JSON.stringify({ ...rest, notes, commitCount: commits?.length ?? 0 }, null, 2)}\n`,
  );
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(
      process.env.GITHUB_OUTPUT,
      `skip=${plan.skip}\nversion=${plan.version}\ntag=${plan.tag}\n`,
    );
  }
}

function releaseExists(tag) {
  const result = spawnSync("gh", ["release", "view", tag, "--json", "tagName"], {
    cwd: ROOT,
    encoding: "utf8",
  });
  return result.status === 0;
}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

async function commitRelease() {
  for (let attempt = 1; attempt <= 5; attempt++) {
    git(["fetch", "origin", "main"]);
    git(["checkout", "-B", "release-work", "origin/main"]);

    const plan = planRelease();
    if (plan.skip) {
      if (releaseExists(plan.tag)) {
        writeReleaseFiles(plan, readChangelogSection(plan.version));
        console.log(`Nothing to release: ${plan.tag} already has a GitHub release.`);
        return plan;
      }
      const notes = readChangelogSection(plan.version);
      writeReleaseFiles({ ...plan, skip: false }, notes);
      console.log(`Resuming ${plan.tag}: the tag exists but the release does not.`);
      return { ...plan, skip: false, resume: true };
    }

    writeVersions(plan.version);
    const existing = existsSync(CHANGELOG) ? readFileSync(CHANGELOG, "utf8") : "";
    writeFileSync(
      CHANGELOG,
      upsertChangelog(existing, changelogSection(plan.version, plan.notes)),
    );
    writeReleaseFiles(plan, plan.notes);

    git([
      "add",
      "--",
      "package.json",
      "apps/desktop/package.json",
      "packages/contract/package.json",
      "package-lock.json",
      "Cargo.toml",
      "Cargo.lock",
      "apps/desktop/src-tauri/tauri.conf.json",
      "CHANGELOG.md",
    ]);
    const staged = spawnSync("git", ["diff", "--cached", "--quiet"], { cwd: ROOT });
    if (staged.status !== 0) {
      git(["commit", "-m", `chore(release): ${plan.tag} [skip ci]`]);
    }

    git(["tag", "-d", plan.tag], { allowFail: true });
    git(["tag", "-a", plan.tag, "-m", plan.tag]);
    const push = git(
      ["push", "--atomic", "origin", "HEAD:main", "--follow-tags"],
      { allowFail: true },
    );
    if (push.ok) {
      console.log(`Released ${plan.tag} (${plan.commits.length} commits).`);
      return plan;
    }

    git(["tag", "-d", plan.tag], { allowFail: true });
    console.warn(
      `Push attempt ${attempt} lost the race with main; retrying in 5s.\n${push.stderr}`,
    );
    await sleep(5000);
  }
  throw new Error("Could not push the release after 5 attempts.");
}

function usage() {
  console.error(
    "Usage: bun scripts/release.mjs [--check | --dry-run | --apply | --commit]",
  );
}

async function main(argv) {
  const mode = ["check", "dry-run", "apply", "commit"].find((name) =>
    argv.includes(`--${name}`),
  );
  if (!mode) {
    usage();
    process.exitCode = 1;
    return;
  }

  if (mode === "check") {
    const version = checkVersions();
    console.log(`Version surfaces agree on ${version}.`);
    return;
  }

  if (mode === "commit") {
    await commitRelease();
    return;
  }

  const plan = planRelease();
  if (plan.skip) {
    writeReleaseFiles(plan, readChangelogSection(plan.version));
    console.log(`Nothing to release: ${plan.tag} is already tagged.`);
    return;
  }

  if (mode === "dry-run") {
    console.log(`${plan.tag} (from ${plan.previousTag ?? "the current package version"})`);
    console.log("");
    console.log(plan.notes);
    return;
  }

  writeVersions(plan.version);
  const existing = existsSync(CHANGELOG) ? readFileSync(CHANGELOG, "utf8") : "";
  writeFileSync(
    CHANGELOG,
    upsertChangelog(existing, changelogSection(plan.version, plan.notes)),
  );
  writeReleaseFiles(plan, plan.notes);
  console.log(`Prepared ${plan.tag}: ${VERSION_FILES.length} version files and CHANGELOG.md.`);
}

const invokedDirectly =
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url;
if (invokedDirectly) await main(process.argv.slice(2));
