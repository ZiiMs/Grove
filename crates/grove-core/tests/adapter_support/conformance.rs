//! The conformance suite every registered adapter must pass.
//!
//! It is adapter-agnostic: a [`Target`] pairs an adapter with a factory that starts a session against
//! its fixture. Scenarios are requested by name; a target maps them to whatever its fixture
//! understands (the in-process mock ignores them):
//!
//! - `turn`       — a normal turn, including a tool call when the adapter claims `tool_calls`
//! - `slow`       — still streaming when the cancel lands
//! - `unknown`    — the transport carries unknown and malformed frames, or the input is junk
//! - `permission` — an approval request the adapter answers; only a target that opts in with
//!   `Target::with_permissions()` and scripts the scenario may claim `capabilities.permissions`
//!
//! The checks are the contract in `architecture.md` §The adapter contract and §Normalized event
//! model: capabilities honesty, ordering and termination, cancel, resume-token roundtrip,
//! malformed-input tolerance, and a permission roundtrip when one is claimed. A sixth adapter adds a
//! target here and keeps these green.

use super::runtime::{deps, drain_until, drain_until_state, is_state, next_event, tags};
use grove_core::adapters::{AdapterError, HarnessAdapter, HarnessSession, StartContext, UserInput};
use grove_core::sessions::event::{Decision, EventKind, HarnessEvent, Role, SessionState};
use std::sync::Arc;

/// One adapter under test plus the fixture that scripts the conformance scenarios.
pub struct Target {
    pub name: &'static str,
    pub adapter: Arc<dyn HarnessAdapter>,
    context: Box<dyn Fn(&str) -> StartContext + Send + Sync>,
    /// Set by [`Target::with_permissions`]: the fixture scripts the `permission` scenario.
    permissions: bool,
}

impl Target {
    /// `context` maps a scenario name (`turn`, `slow`, `unknown`, `permission`) to a start context.
    pub fn new(
        adapter: Arc<dyn HarnessAdapter>,
        context: impl Fn(&str) -> StartContext + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: adapter.id().as_str(),
            adapter,
            context: Box::new(context),
            permissions: false,
        }
    }

    /// Opt in to the permission roundtrip. An adapter may only claim `capabilities.permissions` when
    /// its fixture scripts a `permission` scenario and this flag says so.
    pub fn with_permissions(mut self) -> Self {
        self.permissions = true;
        self
    }

    fn start_context(&self, scenario: &str) -> StartContext {
        (self.context)(scenario)
    }

    async fn session(&self, scenario: &str) -> Box<dyn HarnessSession> {
        self.adapter
            .start(self.start_context(scenario), deps())
            .await
            .unwrap_or_else(|error| fail(self, format!("adapter.start failed: {error}")))
    }
}

fn fail(target: &Target, message: String) -> ! {
    panic!("[{}] {message}", target.name)
}

macro_rules! check {
    ($target:expr, $condition:expr, $($arg:tt)*) => {
        if !$condition {
            fail($target, format!($($arg)*));
        }
    };
}

/// Run every conformance check against one adapter.
pub async fn run_all(target: &Target) {
    capabilities_are_honest(target).await;
    events_are_ordered_and_the_turn_terminates(target).await;
    cancel_stops_a_stream(target).await;
    resume_token_round_trips(target).await;
    malformed_input_is_tolerated(target).await;
    if target.permissions {
        permission_round_trips(target).await;
    }
}

/// A capability is `true` only when the adapter really emits that event kind. An adapter that cannot
/// answer a permission request must say so with `Unsupported`, not pretend.
async fn capabilities_are_honest(target: &Target) {
    let capabilities = target.adapter.capabilities();
    let mut session = target.session("turn").await;
    let mut events_rx = session.take_events();
    session
        .send(UserInput::text("conformance: hello"))
        .await
        .unwrap_or_else(|error| fail(target, format!("send failed: {error}")));
    let observed = drain_until_state(&mut events_rx, SessionState::Idle).await;
    let kinds = tags(&observed);
    let has = |tag: &str| kinds.contains(&tag);

    check!(
        target,
        kinds.first() == Some(&"session_started"),
        "the first event is session_started, got {kinds:?}"
    );
    if capabilities.streaming_deltas {
        check!(
            target,
            has("delta"),
            "streaming_deltas is true but the turn emitted no delta: {kinds:?}"
        );
    }
    if capabilities.tool_calls {
        check!(
            target,
            has("tool_call") && has("tool_result"),
            "tool_calls is true but the turn emitted no tool_call/tool_result pair: {kinds:?}"
        );
    }
    if capabilities.diffs {
        check!(
            target,
            has("diff"),
            "diffs is true but the turn emitted no diff: {kinds:?}"
        );
    }
    if capabilities.usage {
        check!(
            target,
            has("usage"),
            "usage is true but the turn emitted no usage: {kinds:?}"
        );
    }
    if capabilities.resume {
        check!(
            target,
            session.resume_token().is_some(),
            "resume is true but the session has no token after a turn"
        );
    }
    if capabilities.permissions {
        // A claim is only accepted when the fixture scripts a roundtrip for it.
        check!(
            target,
            target.permissions,
            "claims interactive permissions without a scripted roundtrip: add a `permission` scenario \
             to this suite and opt in with Target::with_permissions() before claiming it"
        );
    } else {
        let error = session
            .respond_permission("perm-1", Decision::Approve)
            .await
            .expect_err("an adapter without permission support must reject respond_permission");
        check!(
            target,
            matches!(error, AdapterError::Unsupported(_)),
            "an adapter without permission support reports Unsupported, got {error:?}"
        );
    }

    let _ = session.shutdown().await;
}

/// The transcript reads top to bottom and a turn ends exactly once.
async fn events_are_ordered_and_the_turn_terminates(target: &Target) {
    let mut session = target.session("turn").await;
    let mut events_rx = session.take_events();
    session
        .send(UserInput::text("conformance: a normal turn"))
        .await
        .unwrap_or_else(|error| fail(target, format!("send failed: {error}")));
    let events = drain_until_state(&mut events_rx, SessionState::Idle).await;
    let kinds = tags(&events);

    check!(
        target,
        kinds.first() == Some(&"session_started"),
        "the transcript opens with session_started: {kinds:?}"
    );
    check!(
        target,
        kinds
            .iter()
            .filter(|tag| **tag == "session_started")
            .count()
            == 1,
        "exactly one session_started per session: {kinds:?}"
    );
    match &events[0].kind {
        EventKind::SessionStarted { harness, .. } => check!(
            target,
            harness == target.name,
            "session_started names the adapter: {harness} != {}",
            target.name
        ),
        other => fail(
            target,
            format!("the first event is session_started: {other:?}"),
        ),
    }
    check!(
        target,
        events
            .iter()
            .all(|event| event.at.ends_with('Z') && event.at.len() >= 20),
        "every event carries an RFC3339 UTC timestamp"
    );

    let ready = events
        .iter()
        .position(|event| is_state(event, SessionState::Ready))
        .unwrap_or_else(|| {
            fail(
                target,
                format!("the turn never reported status ready: {kinds:?}"),
            )
        });
    let content = |event: &HarnessEvent| {
        matches!(
            event.kind,
            EventKind::Delta { .. } | EventKind::Message { .. } | EventKind::ToolCall { .. }
        )
    };
    let first_content = events
        .iter()
        .position(content)
        .unwrap_or_else(|| fail(target, format!("the turn produced no content: {kinds:?}")));
    check!(
        target,
        ready < first_content,
        "status ready precedes content: {kinds:?}"
    );

    if target.adapter.capabilities().streaming_deltas {
        let delta = events
            .iter()
            .position(|event| matches!(event.kind, EventKind::Delta { .. }))
            .unwrap_or_else(|| fail(target, format!("no delta in the turn: {kinds:?}")));
        let message = events
            .iter()
            .position(|event| {
                matches!(
                    &event.kind,
                    EventKind::Message {
                        role: Role::Assistant,
                        ..
                    }
                )
            })
            .unwrap_or_else(|| fail(target, format!("no assistant message: {kinds:?}")));
        check!(
            target,
            delta < message,
            "the streamed delta precedes the message that completes it: {kinds:?}"
        );
    }

    if let Some((call_at, answered)) =
        events
            .iter()
            .enumerate()
            .find_map(|(index, event)| match &event.kind {
                EventKind::ToolCall { call_id, .. } => Some((index, call_id.clone())),
                _ => None,
            })
    {
        let result_at = events.iter().position(|event| {
            matches!(&event.kind, EventKind::ToolResult { call_id, .. } if *call_id == answered)
        });
        match result_at {
            Some(result_at) => check!(
                target,
                call_at < result_at,
                "the tool call precedes its result ({answered}): {kinds:?}"
            ),
            None => fail(
                target,
                format!("tool call {answered} was never answered by a tool_result: {kinds:?}"),
            ),
        }
    }

    let last = events.last().map(|event| event.kind.tag());
    check!(
        target,
        last == Some("status"),
        "the turn terminates with an event: {kinds:?}"
    );
    check!(
        target,
        is_state(
            events.last().expect("non-empty transcript"),
            SessionState::Idle
        ),
        "the turn terminates idle: {kinds:?}"
    );
    check!(
        target,
        !events
            .iter()
            .any(|event| is_state(event, SessionState::Exited)),
        "a completed turn is idle, not exited: {kinds:?}"
    );

    let _ = session.shutdown().await;
}

/// Cancel stops the stream, is not an error, and leaves the session usable.
async fn cancel_stops_a_stream(target: &Target) {
    let mut session = target.session("slow").await;
    let mut events_rx = session.take_events();
    session
        .send(UserInput::text("conformance: a long slow turn"))
        .await
        .unwrap_or_else(|error| fail(target, format!("send failed: {error}")));
    let startup = drain_until(&mut events_rx, |event| {
        matches!(event.kind, EventKind::Delta { .. })
    })
    .await;
    check!(
        target,
        startup
            .iter()
            .any(|event| matches!(event.kind, EventKind::Delta { .. })),
        "the slow scenario streams before the cancel"
    );

    session
        .cancel()
        .await
        .unwrap_or_else(|error| fail(target, format!("cancel failed: {error}")));
    let after = drain_until_state(&mut events_rx, SessionState::Idle).await;
    check!(
        target,
        !after
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "a cancel is not an error: {:?}",
        tags(&after)
    );
    check!(
        target,
        !startup.iter().chain(after.iter()).any(|event| matches!(
            event.kind,
            EventKind::Message {
                role: Role::Assistant,
                ..
            }
        )),
        "a cancelled turn emits no assistant message: {:?}",
        tags(&startup)
            .into_iter()
            .chain(tags(&after))
            .collect::<Vec<_>>()
    );

    session
        .send(UserInput::text("conformance: after the cancel"))
        .await
        .unwrap_or_else(|error| fail(target, format!("send after cancel failed: {error}")));
    let next = drain_until_state(&mut events_rx, SessionState::Idle).await;
    check!(
        target,
        next.iter().any(|event| matches!(
            event.kind,
            EventKind::Message { .. } | EventKind::Delta { .. }
        )),
        "the session keeps working after a cancel: {:?}",
        tags(&next)
    );

    let _ = session.shutdown().await;
}

/// A conversation id survives a restart, and the resumed session reports `resumed: true`.
async fn resume_token_round_trips(target: &Target) {
    let mut first = target.session("turn").await;
    let mut first_rx = first.take_events();
    first
        .send(UserInput::text("conformance: first turn"))
        .await
        .unwrap_or_else(|error| fail(target, format!("send failed: {error}")));
    drain_until_state(&mut first_rx, SessionState::Idle).await;
    let token = first
        .resume_token()
        .unwrap_or_else(|| fail(target, "no resume token after a turn".to_string()));
    let _ = first.shutdown().await;

    let mut context = target.start_context("turn");
    context.resume_token = Some(token.clone());
    let mut second = target
        .adapter
        .start(context, deps())
        .await
        .unwrap_or_else(|error| fail(target, format!("resume start failed: {error}")));
    let mut second_rx = second.take_events();
    let started = next_event(&mut second_rx).await;
    match &started.kind {
        EventKind::SessionStarted { resumed, .. } => check!(
            target,
            *resumed,
            "a resumed session reports resumed: true: {started:?}"
        ),
        other => fail(
            target,
            format!("the first event is session_started: {other:?}"),
        ),
    }
    check!(
        target,
        second.resume_token().as_deref() == Some(token.as_str()),
        "the token is available before the first turn: {:?} != {token}",
        second.resume_token()
    );

    second
        .send(UserInput::text("conformance: resumed turn"))
        .await
        .unwrap_or_else(|error| fail(target, format!("send failed: {error}")));
    drain_until_state(&mut second_rx, SessionState::Idle).await;
    check!(
        target,
        second.resume_token().as_deref() == Some(token.as_str()),
        "the harness kept the same conversation id after resuming: {:?} != {token}",
        second.resume_token()
    );

    let _ = second.shutdown().await;
}

/// Unknown frames and junk input are logged and skipped, never fatal.
async fn malformed_input_is_tolerated(target: &Target) {
    let mut session = target.session("unknown").await;
    let mut events_rx = session.take_events();
    let junk = "line one\n{\"type\":\"from-the-future\",\"session_id\":\"x\"}\n\u{2028}odd\n";
    session
        .send(UserInput::text(junk))
        .await
        .unwrap_or_else(|error| fail(target, format!("send failed: {error}")));
    let events = drain_until_state(&mut events_rx, SessionState::Idle).await;
    let kinds = tags(&events);
    check!(
        target,
        !events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "unknown or malformed frames are skipped, never fatal: {kinds:?}"
    );
    check!(
        target,
        events.iter().any(|event| matches!(
            event.kind,
            EventKind::Delta { .. } | EventKind::Message { .. }
        )),
        "the valid frames still reach the transcript: {kinds:?}"
    );

    session
        .send(UserInput::text("still alive"))
        .await
        .unwrap_or_else(|error| fail(target, format!("send after junk failed: {error}")));
    let more = drain_until_state(&mut events_rx, SessionState::Idle).await;
    check!(
        target,
        more.iter().any(|event| matches!(
            event.kind,
            EventKind::Message { .. } | EventKind::Delta { .. }
        )),
        "the session still works after malformed input: {:?}",
        tags(&more)
    );

    let _ = session.shutdown().await;
}

/// An approval request reaches the transcript, the answer is delivered to the harness, and the same
/// request resolves. The fixture refuses to finish the turn until it receives a valid decision, so a
/// stubbed `respond_permission` cannot pass this check.
async fn permission_round_trips(target: &Target) {
    let mut session = target.session("permission").await;
    let mut events_rx = session.take_events();
    session
        .send(UserInput::text("conformance: approve a command"))
        .await
        .unwrap_or_else(|error| fail(target, format!("send failed: {error}")));

    let events = drain_until(&mut events_rx, |event| {
        matches!(event.kind, EventKind::PermissionRequest { .. })
    })
    .await;
    let (request_id, title, detail) = events
        .iter()
        .find_map(|event| match &event.kind {
            EventKind::PermissionRequest {
                request_id,
                title,
                detail,
            } => Some((request_id.clone(), title.clone(), detail.clone())),
            _ => None,
        })
        .expect("the drain stopped on a permission request");
    check!(
        target,
        !request_id.is_empty(),
        "a permission request carries an id the UI can answer"
    );
    check!(
        target,
        !title.is_empty(),
        "a permission request has a human-readable title: {request_id}"
    );
    check!(
        target,
        !detail.is_empty(),
        "a permission request explains what is being asked: {request_id}"
    );

    session
        .respond_permission(&request_id, Decision::Approve)
        .await
        .unwrap_or_else(|error| fail(target, format!("respond_permission failed: {error}")));

    let after = drain_until_state(&mut events_rx, SessionState::Idle).await;
    let kinds = tags(&after);
    check!(
        target,
        after.iter().any(|event| matches!(
            &event.kind,
            EventKind::PermissionResolved {
                request_id: resolved,
                decision: Decision::Approve,
            } if *resolved == request_id
        )),
        "the answer resolves the same request as approved: {kinds:?}"
    );
    check!(
        target,
        !after
            .iter()
            .any(|event| matches!(event.kind, EventKind::Error { .. })),
        "an answered approval is not an error: {kinds:?}"
    );
    check!(
        target,
        after.iter().any(|event| matches!(
            event.kind,
            EventKind::Message { .. } | EventKind::Delta { .. }
        )),
        "the turn continues after the approval: {kinds:?}"
    );

    let _ = session.shutdown().await;
}
