use grove_core::gitops::{
    DiffTarget, GitBranch, GitDiff, GitStatus, GitStatusFile, GitWorktree, PrResult,
};
use grove_core::ipc::{
    AppInfo, CreateSessionRequest, DirEntry, DirListing, ErrorCode, EventEnvelope, IpcError,
    Project, ProjectDetection, Session, SessionDetail, SessionSummary,
};
use grove_core::plan::{Plan, PlanUpdate};
use grove_core::providers::{Capabilities, Detection};
use grove_core::sessions::context::{ContextLimits, ContextPin, SessionContext};
use grove_core::sessions::event::{
    Block, ContextDocument, ContextReason, Decision, EventKind, Question, QuestionAnswer,
    QuestionChoice, QuestionOption, Role, SessionEvent, SessionState, ToolStatus,
};
use grove_core::skills::InstallRecord;

/// Each fixture is canonical JSON produced by the Rust side. A change to a DTO without a matching
/// fixture change fails here, and the TS side fails too — that is the contract test.
fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract/fixtures")
        .join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read fixture {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("fixture is valid JSON")
}

#[test]
fn app_info_matches_fixture() {
    let sample = AppInfo {
        name: "Grove".into(),
        version: "0.1.0".into(),
        app_data_dir: "C:/Users/demo/AppData/Roaming/Grove".into(),
        platform: "windows".into(),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("app-info.json")
    );
}

#[test]
fn event_envelope_matches_fixture() {
    let sample = EventEnvelope::session(
        "session-1",
        serde_json::json!({ "kind": "status", "state": "idle" }),
    );
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("event-envelope.json")
    );
}

#[test]
fn ipc_error_matches_fixture() {
    let sample = IpcError {
        code: ErrorCode::Conflict,
        message: "plan changed on disk".into(),
        detail: Some("expected version 3".into()),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("ipc-error.json")
    );
}

#[test]
fn capabilities_matches_fixture() {
    // Capability honesty, pinned: Claude's honest set (no interactive permissions in one-shot mode,
    // no synthesized diffs yet) rather than an aspirational all-true.
    let sample = Capabilities {
        streaming_deltas: true,
        tool_calls: true,
        diffs: false,
        permissions: false,
        resume: true,
        images: false,
        mcp: false,
        usage: true,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("capabilities.json")
    );
}

#[test]
fn detection_matches_fixture() {
    let sample = Detection {
        installed: true,
        method: "api_key".into(),
        default_model: "claude-sonnet-4-5".into(),
        detail: None,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("detection.json")
    );
}

fn sample_spec_node() -> grove_core::specgraph::SpecNode {
    grove_core::specgraph::SpecNode {
        id: "grove-architecture".into(),
        spec_type: "architecture-design".into(),
        title: Some("Grove — architecture".into()),
        status: Some("active".into()),
        path: "architecture.md".into(),
        parent: Some("grove-goal-and-requirements".into()),
        depends_on: vec!["grove-goal-and-requirements".into()],
        references: vec!["grove-ipc-contract".into()],
        implements: vec!["grove-goal-and-requirements".into()],
        covers: vec!["specgraph".into()],
    }
}

fn sample_spec_validation() -> grove_core::specgraph::SpecValidation {
    grove_core::specgraph::SpecValidation {
        dangling_links: vec![grove_core::specgraph::SpecDanglingLink {
            from: "grove-architecture".into(),
            path: "architecture.md".into(),
            field: "references".into(),
            target: "grove-missing-spec".into(),
        }],
        duplicate_ids: vec![grove_core::specgraph::SpecDuplicateId {
            id: "grove-spec".into(),
            paths: vec!["specs/one.md".into(), "specs/two.md".into()],
        }],
        parent_cycles: vec![grove_core::specgraph::SpecParentCycle {
            ids: vec!["cycle-a".into(), "cycle-b".into()],
            paths: vec!["specs/cycle-a.md".into(), "specs/cycle-b.md".into()],
        }],
    }
}

#[test]
fn spec_node_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_spec_node()).unwrap(),
        fixture("spec-node.json")
    );
}

#[test]
fn spec_validation_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_spec_validation()).unwrap(),
        fixture("spec-validation.json")
    );
}

#[test]
fn spec_graph_matches_fixture() {
    let sample = grove_core::specgraph::SpecGraph {
        root: "C:/demo/grove".into(),
        nodes: vec![sample_spec_node()],
        validation: grove_core::specgraph::SpecValidation::default(),
    };
    assert_eq!(
        serde_json::to_value(sample).unwrap(),
        fixture("spec-graph.json")
    );
}

#[test]
fn spec_doc_matches_fixture() {
    let sample = grove_core::specgraph::SpecDoc {
        node: sample_spec_node(),
        body: "# Grove — architecture\n\nSettles the open questions.\n".into(),
        hash: "9f2c4e1a8b7d6c5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e".into(),
    };
    assert_eq!(
        serde_json::to_value(sample).unwrap(),
        fixture("spec-doc.json")
    );
}

#[test]
fn spec_change_matches_fixture() {
    let sample = grove_core::specgraph::SpecChange {
        paths: vec!["architecture.md".into(), "goal-and-requirements.md".into()],
    };
    assert_eq!(
        serde_json::to_value(sample).unwrap(),
        fixture("spec-change.json")
    );
}
fn sample_git_status() -> GitStatus {
    GitStatus {
        repo_root: "C:/work/grove".into(),
        branch: Some("ticket/04-git-panel".into()),
        head: Some("9f2c1b7d4e5a6c8b0d1e2f3a4b5c6d7e8f901234".into()),
        clean: false,
        truncated: false,
        files: vec![
            GitStatusFile {
                path: "src/main.ts".into(),
                from: None,
                index: "M".into(),
                worktree: " ".into(),
                staged: true,
                untracked: false,
            },
            GitStatusFile {
                path: "docs/notes.md".into(),
                from: None,
                index: " ".into(),
                worktree: "M".into(),
                staged: false,
                untracked: false,
            },
            GitStatusFile {
                path: "src/renamed.ts".into(),
                from: Some("src/old.ts".into()),
                index: "R".into(),
                worktree: " ".into(),
                staged: true,
                untracked: false,
            },
            GitStatusFile {
                path: "scratch.txt".into(),
                from: None,
                index: "?".into(),
                worktree: "?".into(),
                staged: false,
                untracked: true,
            },
        ],
    }
}

#[test]
fn git_status_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_git_status()).unwrap(),
        fixture("git-status.json")
    );
}

#[test]
fn git_branch_matches_fixture() {
    let sample = GitBranch {
        name: "ticket/04-git-panel".into(),
        remote: None,
        current: true,
        upstream: Some("origin/ticket/04-git-panel".into()),
        ahead: 2,
        behind: 1,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("git-branch.json")
    );
}

#[test]
fn git_worktree_matches_fixture() {
    let sample = GitWorktree {
        path: "C:/work/grove".into(),
        head: Some("9f2c1b7d4e5a6c8b0d1e2f3a4b5c6d7e8f901234".into()),
        branch: Some("ticket/04-git-panel".into()),
        bare: false,
        detached: false,
        current: true,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("git-worktree.json")
    );
}

#[test]
fn git_diff_matches_fixture() {
    let sample = GitDiff {
        repo_root: "C:/work/grove".into(),
        target: DiffTarget::WorkingTree,
        path: Some("src/main.ts".into()),
        base: None,
        branch: None,
        patch: concat!(
            "diff --git a/src/main.ts b/src/main.ts\n",
            "index 1a2b3c4..5d6e7f8 100644\n",
            "--- a/src/main.ts\n",
            "+++ b/src/main.ts\n",
            "@@ -1,2 +1,2 @@\n",
            "-const answer = 41;\n",
            "+const answer = 42;\n",
            " console.log(answer);\n",
        )
        .into(),
        truncated: false,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("git-diff.json")
    );
}

#[test]
fn git_pr_matches_fixture() {
    let sample = PrResult {
        url: "https://github.com/acme/grove/pull/42".into(),
        draft: true,
        base: Some("main".into()),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("git-pr.json")
    );
}

#[test]
fn git_changed_event_matches_fixture() {
    let sample = EventEnvelope::global(sample_git_status());
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("git-changed.json")
    );
}

// ---------- sessions (ticket 02) ----------
//
// One fixture per session DTO, plus one covering every `SessionEvent` payload variant. The same files
// are parsed by the zod schemas in `packages/contract`, so either side drifting breaks a test.

fn sample_session() -> Session {
    Session {
        id: "session-1".into(),
        provider: "mock".into(),
        cwd: "C:/projects/grove".into(),
        model: Some("mock-1".into()),
        title: "Refactor the store".into(),
        status: SessionState::Idle,
        project_id: None,
        awaiting: false,
        created_at: "2025-01-01T00:00:00.000Z".into(),
        updated_at: "2025-01-01T00:00:05.000Z".into(),
    }
}

fn sample_events() -> Vec<SessionEvent> {
    vec![
        event(
            EventKind::SessionStarted {
                provider: "mock".into(),
                model: Some("mock-1".into()),
                resumed: false,
            },
            "2025-01-01T00:00:00.500Z",
        ),
        event(
            EventKind::Status {
                state: SessionState::Ready,
            },
            "2025-01-01T00:00:00.600Z",
        ),
        event(
            EventKind::Delta {
                text: "Mock ".into(),
            },
            "2025-01-01T00:00:01.000Z",
        ),
        event(
            EventKind::Message {
                role: Role::Assistant,
                blocks: vec![
                    Block::Text {
                        text: "Mock reply: hello".into(),
                    },
                    Block::Thinking {
                        text: "the mock has nothing to think about".into(),
                    },
                    Block::Code {
                        language: "rust".into(),
                        text: "fn main() {}".into(),
                    },
                ],
            },
            "2025-01-01T00:00:01.500Z",
        ),
        event(
            EventKind::ToolCall {
                call_id: "mock-call-1".into(),
                name: "echo".into(),
                title: "echo hello".into(),
            },
            "2025-01-01T00:00:02.000Z",
        ),
        event(
            EventKind::ToolResult {
                call_id: "mock-call-1".into(),
                status: ToolStatus::Ok,
                summary: "echoed 5 chars".into(),
            },
            "2025-01-01T00:00:02.500Z",
        ),
        event(
            EventKind::Diff {
                path: "src/main.rs".into(),
                patch: "@@ -1 +1 @@\n-old\n+new".into(),
                added: 1,
                removed: 1,
            },
            "2025-01-01T00:00:03.000Z",
        ),
        event(
            EventKind::PermissionRequest {
                request_id: "req-1".into(),
                title: "Run a command".into(),
                detail: "rm -rf build".into(),
            },
            "2025-01-01T00:00:03.500Z",
        ),
        event(
            EventKind::PermissionResolved {
                request_id: "req-1".into(),
                decision: Decision::Deny,
            },
            "2025-01-01T00:00:03.600Z",
        ),
        event(
            EventKind::QuestionRequest {
                request_id: "question-call-1".into(),
                questions: vec![
                    Question {
                        id: "scope".into(),
                        title: "Which scope should this change cover?".into(),
                        hint: "This decides how much I touch before handing back.".into(),
                        multi: false,
                        other: true,
                        options: vec![
                            QuestionOption {
                                label: "Session module only".into(),
                                description: "Smallest diff.".into(),
                            },
                            QuestionOption {
                                label: "Session module + adapter trait".into(),
                                description: "Lets other providers reuse the loop.".into(),
                            },
                        ],
                    },
                    Question {
                        id: "verify".into(),
                        title: "How should I verify it?".into(),
                        hint: "Pick any that apply.".into(),
                        multi: true,
                        other: true,
                        options: vec![QuestionOption {
                            label: "Run the crate tests".into(),
                            description: "cargo test -p grove-auth oauth".into(),
                        }],
                    },
                ],
            },
            "2025-01-01T00:00:03.700Z",
        ),
        event(
            EventKind::QuestionResolved {
                request_id: "question-call-1".into(),
                answers: vec![
                    QuestionAnswer {
                        question_id: "scope".into(),
                        answers: vec![QuestionChoice {
                            label: "Session module only".into(),
                            note: Some("Keep the diff small".into()),
                        }],
                    },
                    QuestionAnswer {
                        question_id: "verify".into(),
                        answers: vec![QuestionChoice {
                            label: "Run the crate tests".into(),
                            note: None,
                        }],
                    },
                ],
            },
            "2025-01-01T00:00:03.800Z",
        ),
        event(
            EventKind::Usage {
                input_tokens: 5,
                output_tokens: 4,
            },
            "2025-01-01T00:00:04.000Z",
        ),
        event(
            EventKind::Status {
                state: SessionState::Idle,
            },
            "2025-01-01T00:00:04.500Z",
        ),
        event(
            EventKind::Error {
                message: "provider exited".into(),
                retryable: true,
            },
            "2025-01-01T00:00:05.000Z",
        ),
        event(
            EventKind::ContextRefreshed {
                reason: ContextReason::Refresh,
                provider: "mock".into(),
                documents: vec![ContextDocument {
                    id: "grove-architecture".into(),
                    title: "Grove — architecture".into(),
                    chars: 1200,
                    truncated: false,
                }],
                note: "Context refreshed for mock: 1 document (1200 chars)".into(),
            },
            "2025-01-01T00:00:06.000Z",
        ),
    ]
}

fn event(kind: EventKind, at: &str) -> SessionEvent {
    SessionEvent::at(kind, at)
}

#[test]
fn session_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_session()).unwrap(),
        fixture("session.json")
    );
}

#[test]
fn session_summary_matches_fixture() {
    let sample = SessionSummary {
        session: sample_session(),
        event_count: 12,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("session-summary.json")
    );
}

#[test]
fn session_detail_matches_fixture() {
    let events = sample_events();
    let sample = SessionDetail {
        session: sample_session(),
        events: vec![events[0].clone(), events[3].clone()],
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("session-detail.json")
    );
}

#[test]
fn every_harness_event_variant_matches_the_fixture() {
    let events = sample_events();
    assert_eq!(
        serde_json::to_value(&events).unwrap(),
        fixture("session-events.json")
    );

    let tags: Vec<&str> = events.iter().map(|event| event.kind.tag()).collect();
    for required in [
        "session_started",
        "delta",
        "message",
        "tool_call",
        "tool_result",
        "diff",
        "permission_request",
        "permission_resolved",
        "question_request",
        "question_resolved",
        "usage",
        "status",
        "error",
        "context_refreshed",
    ] {
        assert!(tags.contains(&required), "fixture misses {required}");
    }
}

fn sample_plan() -> Plan {
    Plan {
        session_id: "session-1".into(),
        markdown: "# Plan\n\n- ship ticket 09\n".into(),
        version: 3,
        mirror_path: "C:/projects/grove/.grove/sessions/session-1/plan.md".into(),
    }
}

#[test]
fn plan_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_plan()).unwrap(),
        fixture("plan.json")
    );
}

#[test]
fn plan_update_matches_fixture() {
    let sample = PlanUpdate {
        markdown: "# Plan\n\n- ship ticket 09\n".into(),
        expected_version: 3,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("plan-update.json")
    );
}

// ---------- session knowledge (ticket 14) ----------

fn sample_session_context() -> SessionContext {
    SessionContext {
        session_id: "session-1".into(),
        provider: "claude".into(),
        pins: vec![
            ContextPin {
                spec_id: "grove-architecture".into(),
                title: "Grove — architecture".into(),
            },
            ContextPin {
                spec_id: "grove-goal-and-requirements".into(),
                title: "Grove — goal and requirements".into(),
            },
        ],
        documents: vec![
            ContextDocument {
                id: "grove-architecture".into(),
                title: "Grove — architecture".into(),
                chars: 1200,
                truncated: false,
            },
            ContextDocument {
                id: "grove-goal-and-requirements".into(),
                title: "Grove — goal and requirements".into(),
                chars: 480,
                truncated: true,
            },
            ContextDocument {
                id: "plan".into(),
                title: "Plan".into(),
                chars: 96,
                truncated: false,
            },
        ],
        total_chars: 1776,
        limits: ContextLimits {
            per_doc_chars: 8000,
            total_chars: 24000,
        },
        plan_version: 3,
    }
}

#[test]
fn session_context_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_session_context()).unwrap(),
        fixture("session-context.json")
    );
}

#[test]
fn plan_changed_envelope_matches_fixture() {
    let sample = EventEnvelope::session("session-1", sample_plan());
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("plan-changed.json")
    );
}

#[test]
fn provider_info_matches_fixture() {
    // The picker's row: what a provider is, what it costs to start, and the models it offers.
    let sample = grove_core::providers::catalog::find("anthropic").expect("anthropic ships");
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("provider-info.json")
    );
}

#[test]
fn create_session_request_matches_fixture() {
    // `config` is provider config from settings; the zod schema had drifted without it, so this
    // fixture is what keeps the two sides honest.
    let sample = CreateSessionRequest {
        provider: "anthropic".into(),
        cwd: Some("C:/work/grove".into()),
        model: Some("claude-sonnet-4-5".into()),
        title: Some("Review the diff".into()),
        project_id: None,
        worktree_path: None,
        config: Some(serde_json::json!({ "maxSteps": 12 })),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("create-session-request.json")
    );
}

// ---------- workspace (redesign) ----------

fn sample_project() -> Project {
    Project {
        id: "project-1".into(),
        name: "grove".into(),
        path: "C:/projects/grove".into(),
        remote: Some("acme/grove".into()),
        default_branch: Some("main".into()),
        is_git: true,
        created_at: "2025-01-01T00:00:00.000Z".into(),
        updated_at: "2025-01-01T00:00:00.000Z".into(),
    }
}

#[test]
fn project_matches_fixture() {
    assert_eq!(
        serde_json::to_value(sample_project()).unwrap(),
        fixture("project.json")
    );
}

#[test]
fn project_detection_matches_fixture() {
    let sample = ProjectDetection {
        path: "C:/projects/grove".into(),
        is_git: true,
        current_branch: Some("feat/ui".into()),
        branches: vec!["feat/ui".into(), "main".into()],
        default_branch: Some("main".into()),
        remote: Some("acme/grove".into()),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("project-detection.json")
    );
}

#[test]
fn dir_listing_matches_fixture() {
    let sample = DirListing {
        path: "C:/Users/ada/code".into(),
        parent: Some("C:/Users/ada".into()),
        dirs: vec![
            DirEntry {
                name: "agent-hub".into(),
                path: "C:/Users/ada/code/agent-hub".into(),
            },
            DirEntry {
                name: "grove".into(),
                path: "C:/Users/ada/code/grove".into(),
            },
        ],
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("dir-listing.json")
    );
}

#[test]
fn install_record_matches_fixture() {
    let sample = InstallRecord {
        skill: "provider-adapter".into(),
        source: "anthropics/skills".into(),
        target: "agents".into(),
        path: "C:/projects/grove/.agents/skills/provider-adapter".into(),
        installed_at: "2025-01-01T00:00:00Z".into(),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("skill-install-record.json")
    );
}
