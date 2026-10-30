# Releases and updates

Grove releases on every merge to `main`. The pipeline is two workflows and two scripts, and the git
tag is the version of record.

## Workflows

| Workflow | Trigger | What it does |
|---|---|---|
| `.github/workflows/ci.yml` | every pull request and push to `main` | install, version check, script tests, typecheck, vitest, webview build + bundle guard, rustfmt, clippy, nextest, doctests, desktop check; on `main` it then calls the release workflow |
| `.github/workflows/release.yml` | called by CI after the checks on `main` (or manual dispatch) | bump version, write `CHANGELOG.md`, commit/tag/push, build the signed NSIS installer, publish the GitHub Release |

CI runs on `windows-latest` because Windows x86_64 is the supported build. Rust builds use sccache
with the GitHub Actions cache; tests use nextest.

## Versioning

- **The tag is the truth.** `git tag --merged HEAD --list "v*" --sort=-v:refname`, filtered to strict
  `vX.Y.Z` tags, finds the last release reachable from `HEAD`; the next version is derived from the
  commits after it.
- **Conventional commits drive the bump.** Pre-1.0, `feat` and breaking changes move the minor,
  everything else moves the patch. From 1.0.0, breaking moves the major and `feat` the minor.
- **Five surfaces stay in step:** root `package.json`, `apps/desktop/package.json`,
  `packages/contract/package.json`, the `[workspace.package]` version in `Cargo.toml`, and
  `apps/desktop/src-tauri/tauri.conf.json`. `package-lock.json` and the two workspace entries in
  `Cargo.lock` move with them. `bun run release:check` fails if any surface disagrees.
- **The baseline is `v0.3.0`.** Tags `v0.1.1`–`v0.2.0` belong to the pre-rewrite Go app and are not
  ancestors of the current history, so the release script ignores them. The first run of the release
  workflow tags the landing commit `v0.3.0`; every later merge produces the next version.
- **Release commits are bot commits:** `chore(release): vX.Y.Z [skip ci]`. The script re-reads
  `origin/main` on each attempt and retries the atomic push, so a merge that lands mid-release does
  not lose the bump. A `release-main` concurrency group serialises runs.

`bun run release:dry` prints the next version and notes without touching anything.

## What a release contains

| Asset | Purpose |
|---|---|
| `Grove_<version>_x64-setup.exe` | the NSIS installer, and the updater artifact |
| `Grove_<version>_x64-setup.exe.sig` | the minisign signature over the installer |
| `latest.json` | the updater manifest for `windows-x86_64` |
| `SHA256SUMS` | SHA-256 for the installer |

`latest.json` is served from the release:

```
https://github.com/ZiiMs/Grove/releases/latest/download/latest.json
```

That is the endpoint configured in `apps/desktop/src-tauri/tauri.conf.json`. Because the repository
is public, no extra hosting is needed.

## How the updater decides

1. The app calls `check()` (Settings → About). The plugin fetches `latest.json`.
2. It compares the manifest's `version` with the running app version. Equal or older means no update.
3. On install it downloads the `windows-x86_64` URL (the NSIS installer) and verifies the
   **minisign signature** against the public key in `tauri.conf.json` before running it.

Checksums and signatures do different jobs:

- `SHA256SUMS` proves a manual download arrived intact. It does not authenticate the publisher and
  the updater does not use it.
- The `.sig` / `latest.json` signature authenticates the update. Without it, the updater refuses to
  install.

## Signing key

The keypair is generated once with:

```sh
bun run --cwd apps/desktop tauri signer generate -w <path-to-key> -p <password>
```

- The **public key** is committed in `apps/desktop/src-tauri/tauri.conf.json` under
  `plugins.updater.pubkey`.
- The **private key file** and its password are GitHub Actions secrets:
  `TAURI_SIGNING_PRIVATE_KEY` (the file content) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Keep an offline copy. If both the key and password are lost, existing installs cannot be updated;
  users have to download a new installer by hand.

**Rotation.** A build verifies updates with the key compiled into it, so a new key only reaches
users through a release signed with the old key. Keep the old key until that release is installed,
then switch `pubkey` and the secrets.

**Local builds** do not need the key: `createUpdaterArtifacts` is turned on only by the release
workflow's `--config` override. `bun run tauri build` still produces an unsigned installer locally.

## Failure recovery

- **Tag pushed, publish failed.** Re-run the CI workflow (or dispatch the release workflow manually).
  The script sees the tag, finds no GitHub release, and resumes from the tagged commit instead of
  bumping again.
- **Push raced with a merge.** The script retries the atomic push up to five times.
- **Bad release.** Prefer a new patch release over deleting a tag; clients may already have seen the
  manifest. If you must delete, remove the GitHub release and tag together, then run the workflow
  again from `main`.
- **Version drift.** `bun run release:check` names the file that disagrees.

## Branch protection

The release job pushes its bump commit and tag with `GITHUB_TOKEN`. If `main` gets protection, allow
GitHub Actions to push (or switch the checkout `token` to a fine-grained PAT stored as a secret) and
keep the `release-main` concurrency group. Do not let a ruleset block tags matching `v*`.

## Later

- macOS (`darwin-aarch64`, `darwin-x86_64`) and Linux bundles, and an MSI alongside NSIS.
- Windows Authenticode signing for the installer itself, separate from the updater signature.
- A beta channel by pointing a second manifest at pre-releases.
- A silent background check with a non-blocking toast; the plugin and UI already support the manual
  path.
- Delta updates once the installer is large enough to justify them.
