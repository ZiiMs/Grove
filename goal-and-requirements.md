---
id: grove-goal-and-requirements
type: goal-and-requirements
status: active
title: Grove — one desktop home for every AI coding provider
tags: [desktop, agent-harness, spec-driven-development, tauri]
---

# Grove

**One desktop home for every AI coding model - with the project's intent, not just its chat, kept in view.**

> **Amended 2026-10-25.** This document is the original intent of record and is kept as written. Where
> it specifies harness adapters, a sidecar and per-harness skill directories, ADR 0003 supersedes it:
> Grove now runs the agent itself against model providers (Anthropic, OpenAI, Google, OpenRouter,
> OpenCode Go) with an API key. Read `architecture.md` for what is built; read this for why.

## Overview

Grove is an open-source desktop application that runs AI coding agents from several harnesses
(Claude Code, Codex, OpenCode, `pi`, and any ACP-speaking agent) behind one interface, and wraps them in
the layer those harnesses each reinvent badly: a **skill catalog** that installs into whichever harness
you are using, a **spec graph** for spec-driven development, a **TODO plan** that survives restarts, a
**git/PR** view, and an **MCP server viewer**. It is the desktop counterpart to OpenChamber and
ThinkRail: where they go deep on one agent runtime, Grove is broad across runtimes and owns the
project-knowledge layer.

## Problem Statement

A developer doing agentic coding today has to choose a harness *and* a UI for it. Each harness ships its
own terminal or TUI, its own session history, and its own place to keep context: skills live in one
tool's folder, specs follow a convention another tool invented, TODOs sit in a chat scratchpad that
dies with the session. Picking a harness therefore means picking a workflow — and running two side by
side means rebuilding the same project knowledge in each.

The review half of the loop lives somewhere else again: branches, diffs, and PRs sit in GitHub/GitLab,
so the agent that wrote the change never sees the ticket it was for and the human moves between apps to
land it.

## Target Users

Roles, not demographics:

- **The multi-harness developer** — already runs two or more agent CLIs, and resents that each keeps its
  own history and its own idea of where skills and specs go.
- **The context-starved agent user** — good results on small tasks, bad ones on real repos, because the
  agent's knowledge of the project resets every session.
- **The spec-discipline adopter** — wants spec-driven development but not a vendor-specific IDE.

## Jobs to Be Done

- When I have the same task to run and two harnesses that might do it better, I want to start either (or
  both) from one window, so I can compare without rebuilding my context.
- When a session restarts or a different harness takes over, I want the project's skills, specs, and plan
  to still be there, so the agent does not start from zero.
- When a change is ready, I want the branch, the diff, and the PR in the same view, so I can land it
  without switching apps.

## Key User Story

Dev has a repo open in Grove with two agent tabs — one Claude Code session, one Codex. She browses the
skill catalog, picks `code-review`, and installs it to Claude Code's skills directory from a location
dropdown. She writes a two-line TODO plan and pins the relevant spec to both sessions. Codex finishes
first; she opens the diff, sees the spec it was meant to satisfy beside it, and opens a PR from the panel
— without touching a terminal.

## Goals

- Run sessions from five harnesses — Claude Code, Codex, generic ACP, OpenCode, `pi` — in one window on
  day one.
- Get a catalog skill from "found it" to "active in a session" in under 30 seconds, with no terminal.
- Survive a session restart on any harness with the project's skills, specs, and TODO plan intact.
- Take a change from diff to opened PR without leaving Grove.
- Hold idle RAM under ~200 MB, so Grove can sit beside the agents it runs.

## Non-Goals

- **Not a model gateway.** Users bring their own keys or existing CLI auth; Grove never proxies model
  calls and never bills for tokens.
- **Not an IDE or editor replacement.** Monaco appears for diffs and spec files only, never as a
  workspace.

## Success Conditions

- A user who has never run Grove reaches a first agent session from install using only the README.
- An outside contributor adds a sixth harness by implementing one documented adapter interface, without
  touching UI code.
- A session interrupted mid-task resumes with its plan and spec context intact.

## Capabilities

- **Multi-harness sessions** — Claude Code, Codex, generic ACP, OpenCode, and `pi` adapters, each behind
  one interface; sessions side by side, each with its own conversation, tool-call cards, and diffs.
  Providers are integrated through their protocol, never by scraping a terminal.
- **Skill catalog** — browse public GitHub skill catalogs (Claude Code skills, Codex skills, Matt
  Pocock's set, and others), preview a skill, and install it into a chosen harness's own skills
  directory, with a dropdown to choose the target location.
- **Spec graph** — render the project's specs readably, walk their links, edit them in place, and pin
  specs to a session as context. The agent runs the spec-driven workflow; Grove keeps intent visible.
- **TODO plan** — one plan per session, edited by both human and agent, surviving restarts.
- **MCP viewer** — the MCP servers in play, their tools, and their connection state.
- **Git and PRs** — branches, worktrees, diffs, commit, and open a PR in-app. Issue-tracker integration
  (GitHub/GitLab issues, Linear, Jira) is a capability of the product, not of the first build.

## Non-Functional Requirements

- **Local-first.** No account required; project state lives in local files and the repo. Secrets (tokens,
  keys) stay in the OS keychain, never in plaintext config.
- **Offline-capable sessions.** A running session only needs the network for its own provider call;
  the skill catalog and model-capability metadata serve a cache or static fallback when offline.
- **Windows first-class.** Windows is the supported build; macOS/Linux builds are unverified until the
  core is stable.
- **Adapter isolation.** Adding or breaking a harness adapter must not affect the others or the UI shell.

## Technology

| Aspect | Choice | Rationale |
|---|---|---|
| Shell | Tauri 2 | ~10–15 MB installers and low memory; the proven shell for this app class (OpenChamber, Conduit). |
| Core | Rust | Owns filesystem, git, and stdio JSON protocols; keeps agent I/O off a GC. |
| UI | React + TypeScript in the system webview | Fastest iteration for a panel-heavy, diff-heavy interface. |
| Harness access | One adapter per harness protocol: Claude Code `stream-json`/Agent SDK, Codex `app-server` JSON-RPC, OpenCode local HTTP+SSE, `pi` Node SDK, generic ACP | Structured tool calls and diffs need a protocol, not a screen. |
| Sidecar | Node/TS process | Only where a harness exposes a JS-only SDK (`pi`, Claude Agent SDK). |
| Skill install | Per-harness native skills directory, chosen from a dropdown | Skills keep working outside Grove, and no new format is imposed on three harnesses. |

## Notes

- **Open, not excluded:** cloud sync/account and remote/mobile access were neither claimed nor ruled
  out — undecided, not forbidden.
- **Deferred by decision:** issue-tracker integration (below first build, above).
- **To settle in `architecture.md`:** session persistence format, diff/spec rendering component, IPC
  contract between the Rust core, the Node sidecar, and the webview, and the adapter interface shape.

## Alternatives Considered

Every serious entry below already wins "many agents in one window". The gap column is why Grove is not
one of them.

| Project | What it is | Gap Grove fills |
|---|---|---|
| [OpenChamber](https://openchamber.dev/) | Tauri desktop app around OpenCode: git hosts, PR stacks, browser panel, mobile relay, VS Code | One runtime only; no spec graph, skill catalog, or shared TODO layer |
| [ThinkRail](https://thinkrail.ai/) | Agentic IDE around in-process `pi`: worktree workspaces, spec graph, skills, Monaco | Single harness (`pi`); the knowledge layer is not portable to other agents |
| [Harnss](https://github.com/OpenSource03/harnss) | Claude Code, Codex, and ACP agents in one UI with tool-call cards and word diffs | Session multiplexer only — no skills, specs, or plan |
| [Hydra](https://github.com/jpdlr/hydra) | Parallel Claude Code/Codex sessions, live terminals, MCP orchestration | Orchestration without project knowledge; nothing persists across a restart |
| [Conduit](https://github.com/uziiuzair/conduit) | Tauri app embedding real agent CLIs in PTYs, split editor groups | Terminal fidelity over structure — no tool cards, specs, or catalog |
| [desktop-cc-gui](https://github.com/zhukunpenglinyutong/desktop-cc-gui) | Broad multi-engine desktop client (Claude, Codex, Kimi, Grok, pi, OpenCode, …) | Breadth without depth: no spec-driven workflow or knowledge layer |
| [Codemux](https://codemux.org/) | Prompt → branch → worktree → agent, with a browser the agent drives; Linux-first | Strong isolation model, but no spec graph, skills, or shared plan |
| [Agent Moshpit](https://agentmoshpit.com/) | Agents as people at desks in a room, amber band when one needs you | Novel presence UI; thin on project knowledge and git workflow |
