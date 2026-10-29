# Glossary

Only the terms this codebase actually uses. Each one names the module that owns it, so the definition
can be checked against the implementation.

## Project

A folder the user added to the workspace tree (`projects` in SQLite, `grove_core::projects`). It
carries the git facts detected at add time - whether it is a repository, its default branch, its
normalized `owner/name` remote - and a session keeps its `project_id`. A session with no project is a
**chat**: it still runs in a directory, but the tree shows it under `Chats` and the context panel has
no checkout to read.

## Worktree

A `git worktree` checkout of one project on its own branch. Grove never stores worktrees: the tree
reads `git worktree list` per project and groups sessions by the checkout their `cwd` belongs to, so a
worktree created in a terminal appears on the next refresh. A session may run in a worktree instead of
the project folder; `git_worktree_create` is the one place the UI creates a new one.

## Provider

A model vendor Grove talks to directly: Anthropic, OpenAI, Google, OpenRouter or OpenCode Go. A
provider owns its HTTP wire format and nothing else; the three wire clients live in
`crates/grove-core/src/providers/{anthropic,openai,google}.rs`, and a gateway such as OpenCode Go
reuses the client its API speaks. The catalog that names the five lives in `providers/catalog.rs`.
There is no agent CLI to install and no harness to detect - only a credential to configure.

## Model list

The model ids a configured credential can actually reach, fetched by `provider_models`
(`providers/probe.rs`). The webview refreshes every configured provider after boot and after a key
change, replacing the catalog's fallback `models` and moving `defaultModel` when its preset is absent.
A failed probe keeps the catalog list, so an offline start still offers the known models. The live
list is the vendor's, not a list baked into this build.

## Credential

An API key for one provider, stored in the OS keychain through `SecretStore` under
`provider.<id>.api_key` (`providers/credentials.rs`). The vendor's own environment variable
(`ANTHROPIC_API_KEY`, …) is honoured as a fallback, and the keychain always wins. A credential is
never written to `settings`, the database or the logs, and there is no command that returns its value.

## Agent runtime

The loop that runs a turn: ask the provider, stream the text out, run whatever tools the model asked
for, and repeat until it stops (`providers/agent.rs`). It owns the conversation in memory, emits the
normalized events the transcript records, and asks the user before a tool changes the machine.

## Thinking level

A session preference stored as `thinkingLevel` in the session's `adapter_config` and surfaced as
`SessionDetail.thinkingLevel`. The selectable values are model-specific: the core reads models.dev's
per-provider `reasoning_options` into a cached map, so a model can expose `off`/`none`, `minimal`,
`low`, `medium`, `high`, `xhigh`, `max`, or a fallback subset instead of one global ladder.
`session_set_thinking` updates a live agent's next turn and persists the config for a stopped session.
Each wire client maps the selected value to the vendor's native control: Anthropic thinking budget,
OpenAI-compatible `reasoning_effort`, or Google thinking budget. `off` omits the field; `none` is the
explicit `reasoning_effort: "none"` value some models require.

## Tool

One capability the model may call: `read_file`, `list_dir`, `grep`, `write_file`, `edit_file`,
`run_command`, `ask_user` and `subagent` (`providers/tools.rs`). Tools run locally in the session's
working directory, resolve paths against it, and refuse anything that escapes it. The three mutating
tools need approval; `ask_user` parks on the user's answer instead.

## Subagent

A worker session started by the `subagent` tool (`providers/agent.rs`). It shares the provider, model,
key, project directory and pinned context with the parent, but starts a fresh conversation and gets
every tool except `subagent`, so delegation cannot recurse. Its mutating calls still surface as the
parent session's permission cards, and its tool rows and diffs are forwarded; the parent model
receives the worker's final report as the tool result. The pump keeps a `subagents` row (description,
status, summary) so the session tree shows the worker under its parent without reading the transcript.

## Permission request

The card the transcript shows before a mutating tool runs. The runtime emits `permission_request`,
parks the tool, and waits for `session_respond_permission`; it emits `permission_resolved` with the
user's answer and only then proceeds. The buttons disappear when the resolution arrives, so what the
UI shows is what the runtime accepted.

## Question request

The inline card the transcript shows when the model calls `ask_user`. The runtime emits
`question_request` with one or more questions, parks the tool, and waits for
`session_respond_question`; it emits `question_resolved` and returns the structured answers as the
tool result. The card supports single- and multi-select options, per-choice notes, a free-text
"Other" answer, and pages through multiple questions before sending them together.

## Session

A row in `sessions` plus, while it is running, one in-process agent and one pump task
(`sessions::manager`). Sessions are rows, not processes: the transcript is in SQLite, a restart marks a
live row `exited`, and `session_resume` relaunches the agent with the stored resume token or a replay
of the conversation tail. Sending the next message resumes a stopped session automatically; the
conversation survives a quit.

## Archived session

A session whose `sessions.archived_at` is set. It leaves the active tree and appears in the sidebar's
Archive section, keeps its transcript, plan, pins and workers, and can be restored at any time. Only an
archived session can be permanently deleted; archive itself is reversible soft state, never a delete.

## Spec graph

The index of repo `.md` files whose frontmatter carries `id` + `type`, with `parent`, `depends-on`,
`references`, `implements` and `covers` edges (`specgraph/`). It powers the Specs panel, validates
dangling links/duplicate ids/parent cycles, and is the source of the spec node a session can pin.

## Plan mirror

The agent-editable copy of a session's TODO plan at `.grove/sessions/<id>/plan.md` (`plan/`). SQLite is
the record; every write also writes the mirror atomically. `plans.mirror_hash` records what Grove last
wrote, so a read can tell an agent's external edit (import, version bump) from Grove's own write
(rewrite if stale or missing). There is no native file watcher: the check happens at read time.

## Context document

One pinned spec or the current plan, already rendered to markdown, that a session start/resume/refresh
hands to the agent (`providers::ContextDoc`, assembled by `sessions::context::SessionContextService`; the event summary is `sessions::event::ContextDocument`). Context is a **snapshot**
with explicit budgets (8 000 chars per document, 24 000 total) and a truncation marker; nothing
re-reads a spec on a timer. The transcript records what was sent as a `context_refreshed` event.

## Offline provider

The `mock` provider in `providers/offline.rs`: it speaks the real runtime but answers from a script
(chunked deltas, one tool call, usage). Test and preview builds register it alone, so the session
runtime, transcript and cancellation paths are exercised without a key, a network or a vendor. It is
deliberately absent from the shipped catalog.
