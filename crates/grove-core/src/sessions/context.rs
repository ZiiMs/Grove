//! Session knowledge: the pinned specs and the current plan, assembled into the documents a provider
//! receives when a session starts (ticket 14, `architecture.md` §Session runtime).
//!
//! Three rules give this module its shape:
//!
//! - **Specs stay files.** A pin stores the spec *id*; each assembly re-reads the graph and the bodies,
//!   so a spec edited between two starts is picked up by the second one and never by the first.
//! - **Context is a snapshot.** Documents are assembled at start, resume, provider switch or an explicit
//!   refresh — never on a timer — so editing a spec cannot silently change a running conversation.
//! - **No unbounded context.** A per-document and a total character budget bound what a session can
//!   carry, and a document that is cut short says so in its own body with an explicit marker, because
//!   the provider only ever sees text.

use crate::ipc::{IpcError, IpcResult};
use crate::paths::ProjectRoot;
use crate::plan::PlanService;
use crate::providers::ContextDoc;
use crate::sessions::event::ContextDocument;
use crate::specgraph::{read_spec, SpecIndex};
use crate::store::{Store, StoreError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Character budget for one document. A pinned spec is a summary of the project, not the project.
pub const MAX_DOC_CHARS: usize = 8_000;

/// Character budget for everything a session receives at once.
pub const MAX_TOTAL_CHARS: usize = 24_000;

/// A document smaller than this is dropped rather than truncated down to a marker with no content.
pub const MIN_DOC_CHARS: usize = 64;

/// The id the plan document carries. Spec ids come from frontmatter and are never `plan`.
pub const PLAN_DOC_ID: &str = "plan";

/// The explicit marker appended to a truncated document body. It quotes how much was dropped, so the
/// provider can tell a cut-off spec from a short one.
pub fn truncation_marker(omitted: usize, total: usize) -> String {
    format!("\n\n[truncated: {omitted} of {total} chars omitted]")
}

/// The per-document and total character budgets, on the wire so the UI can explain a truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextLimits {
    pub per_doc_chars: usize,
    pub total_chars: usize,
}

impl Default for ContextLimits {
    fn default() -> Self {
        Self {
            per_doc_chars: MAX_DOC_CHARS,
            total_chars: MAX_TOTAL_CHARS,
        }
    }
}

/// One pinned spec, as the session header lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPin {
    pub spec_id: String,
    pub title: String,
}

/// A session's knowledge as the UI shows it: what is pinned, and what a start would send right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    pub session_id: String,
    pub provider: String,
    pub pins: Vec<ContextPin>,
    pub documents: Vec<ContextDocument>,
    pub total_chars: usize,
    pub limits: ContextLimits,
    /// 0 when the session has no plan yet.
    pub plan_version: u64,
}

/// The result of applying the budgets: the documents the provider gets, plus the record the transcript
/// and the header keep.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextAssembly {
    pub docs: Vec<ContextDoc>,
    pub documents: Vec<ContextDocument>,
    /// Documents dropped whole because the total budget was spent.
    pub omitted: usize,
    pub total_chars: usize,
}

/// Where the session runtime gets its context documents. The manager depends on this port, not on the
/// spec graph or the plan service, so its own tests stay headless.
pub trait ContextSource: Send + Sync {
    /// What a session should be started with, budgets applied. Never fails: a missing spec or an
    /// unreadable plan is a smaller context, not a broken session.
    fn documents(&self, session_id: &str) -> ContextAssembly;
}

/// Assemble a session's knowledge from the project's spec files and its plan.
pub struct SessionContextService {
    store: Arc<Store>,
    plan: Arc<PlanService>,
    /// The root the spec graph is indexed from — the same project the Specs panel shows.
    spec_root: ProjectRoot,
    limits: ContextLimits,
}

impl SessionContextService {
    pub fn new(
        store: Arc<Store>,
        plan: Arc<PlanService>,
        spec_root: impl Into<ProjectRoot>,
    ) -> Self {
        Self {
            store,
            plan,
            spec_root: spec_root.into(),
            limits: ContextLimits::default(),
        }
    }

    /// Replace the budgets. Tests use it to prove the caps without building a megabyte of markdown.
    pub fn with_limits(mut self, limits: ContextLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn limits(&self) -> ContextLimits {
        self.limits
    }

    /// The session's knowledge as both the header and a start see it: pins, the documents they produce
    /// under the current budgets, and the plan version.
    pub fn snapshot(&self, session_id: &str) -> IpcResult<SessionContext> {
        let collected = self.collect(session_id)?;
        let assembly = assemble(&collected.docs, self.limits);
        Ok(SessionContext {
            session_id: collected.session.id,
            provider: collected.session.provider,
            pins: collected.pins,
            documents: assembly.documents,
            total_chars: assembly.total_chars,
            limits: self.limits,
            plan_version: collected.plan_version,
        })
    }

    /// Pin a spec to a session and return the refreshed snapshot. Unknown spec ids are rejected: a pin
    /// that can never contribute a document is a typo, not knowledge.
    pub fn pin_spec(&self, session_id: &str, spec_id: &str) -> IpcResult<SessionContext> {
        let session = self.session(session_id)?;
        let spec_id = spec_id.trim();
        if spec_id.is_empty() {
            return Err(IpcError::invalid("a pin needs a spec id"));
        }
        let index = self.index()?;
        if index.node(spec_id).is_none() {
            return Err(IpcError::invalid(format!(
                "no spec with id {spec_id} under {}",
                crate::paths::display_path(&self.spec_root.path())
            )));
        }
        stored(self.store.add_pin(&session.id, spec_id))?;
        self.snapshot(&session.id)
    }

    /// Unpin a spec. Always succeeds for a real session, even when the spec itself is gone: removing a
    /// stale pin must stay possible.
    pub fn unpin_spec(&self, session_id: &str, spec_id: &str) -> IpcResult<SessionContext> {
        let session = self.session(session_id)?;
        stored(self.store.remove_pin(&session.id, spec_id.trim()))?;
        self.snapshot(&session.id)
    }

    /// The budgets applied to the session's documents, with the record the transcript keeps.
    pub fn assembly(&self, session_id: &str) -> IpcResult<ContextAssembly> {
        let collected = self.collect(session_id)?;
        Ok(assemble(&collected.docs, self.limits))
    }

    fn collect(&self, session_id: &str) -> IpcResult<Collected> {
        let session = self.session(session_id)?;
        let index = self.load_index();
        let mut pins = Vec::new();
        let mut docs = Vec::new();
        for spec_id in stored(self.store.pins(&session.id))? {
            let node = index.as_ref().and_then(|index| index.node(&spec_id));
            let title = node
                .and_then(|node| node.title.clone())
                .unwrap_or_else(|| spec_id.clone());
            pins.push(ContextPin {
                spec_id: spec_id.clone(),
                title: title.clone(),
            });
            let (Some(node), Some(index)) = (node, index.as_ref()) else {
                // The spec file is gone: the pin survives, the document cannot.
                continue;
            };
            match read_spec(index.root(), &node.path) {
                Ok(doc) => docs.push(ContextDoc {
                    id: spec_id,
                    title,
                    body: doc.body,
                }),
                Err(error) => tracing::warn!(
                    target: "grove::sessions",
                    spec = %spec_id,
                    "pinned spec is unreadable: {}",
                    error.message
                ),
            }
        }

        // Reading the plan also imports an agent's external edit, which is exactly what a start or a
        // refresh should see. A broken mirror costs the plan document, never the session.
        let mut plan_version = 0;
        match self.plan.get(&session.id) {
            Ok(plan) => {
                plan_version = plan.version;
                if !plan.markdown.trim().is_empty() {
                    docs.push(ContextDoc {
                        id: PLAN_DOC_ID.into(),
                        title: "Plan".into(),
                        body: plan.markdown,
                    });
                }
            }
            Err(error) => tracing::warn!(
                target: "grove::sessions",
                session = session_id,
                "plan is unavailable for the session context: {}",
                error.message
            ),
        }

        Ok(Collected {
            session,
            pins,
            docs,
            plan_version,
        })
    }

    fn session(&self, session_id: &str) -> IpcResult<crate::ipc::Session> {
        stored(self.store.session(session_id))?
            .ok_or_else(|| IpcError::not_found(format!("no session: {session_id}")))
    }

    fn load_index(&self) -> Option<SpecIndex> {
        match SpecIndex::load(self.spec_root.path()) {
            Ok(index) => Some(index),
            Err(error) => {
                tracing::warn!(
                    target: "grove::sessions",
                    "cannot index {} for session context: {}",
                    crate::paths::display_path(&self.spec_root.path()),
                    error.message
                );
                None
            }
        }
    }

    fn index(&self) -> IpcResult<SpecIndex> {
        SpecIndex::load(self.spec_root.path()).map_err(|_| {
            IpcError::io(format!(
                "cannot index pinned specs under {}",
                crate::paths::display_path(&self.spec_root.path())
            ))
        })
    }
}

impl ContextSource for SessionContextService {
    /// The port never fails: an unreadable graph is an empty context and a warning, never a session
    /// that refuses to start.
    fn documents(&self, session_id: &str) -> ContextAssembly {
        match SessionContextService::assembly(self, session_id) {
            Ok(assembly) => assembly,
            Err(error) => {
                tracing::warn!(
                    target: "grove::sessions",
                    session = session_id,
                    "session context is unavailable: {}",
                    error.message
                );
                ContextAssembly::default()
            }
        }
    }
}

struct Collected {
    session: crate::ipc::Session,
    pins: Vec<ContextPin>,
    docs: Vec<ContextDoc>,
    plan_version: u64,
}

/// Apply the budgets. Documents are kept in the order they were assembled (pins, then the plan), each
/// cut to the per-document budget with an explicit marker, and the assembly stops when the total
/// budget is spent — an omitted document is counted, never silently dropped.
pub fn assemble(input: &[ContextDoc], limits: ContextLimits) -> ContextAssembly {
    let per_doc = limits.per_doc_chars.max(MIN_DOC_CHARS);
    let mut assembly = ContextAssembly::default();
    let mut remaining = limits.total_chars;
    for doc in input {
        let budget = per_doc.min(remaining);
        if budget < MIN_DOC_CHARS {
            assembly.omitted += 1;
            continue;
        }
        let (body, truncated) = clamp_body(&doc.body, budget);
        if body.is_empty() {
            assembly.omitted += 1;
            continue;
        }
        let chars = body.chars().count();
        remaining -= chars;
        assembly.total_chars += chars;
        assembly.documents.push(ContextDocument {
            id: doc.id.clone(),
            title: doc.title.clone(),
            chars,
            truncated,
        });
        assembly.docs.push(ContextDoc {
            id: doc.id.clone(),
            title: doc.title.clone(),
            body,
        });
    }
    assembly
}

/// Cut `body` to `budget` characters, marker included. The marker quotes the omitted count, so the
/// loop shrinks the kept prefix until the marker fits inside the same budget.
fn clamp_body(body: &str, budget: usize) -> (String, bool) {
    let total = body.chars().count();
    if total <= budget {
        return (body.to_string(), false);
    }
    let mut keep = total.min(budget);
    loop {
        let marker = truncation_marker(total - keep, total);
        let marker_len = marker.chars().count();
        if keep + marker_len <= budget {
            let mut rendered: String = body.chars().take(keep).collect();
            rendered.push_str(&marker);
            return (rendered, true);
        }
        let over = keep + marker_len - budget;
        if over >= keep {
            // Not even the marker fits: the caller reports the document as omitted.
            return (String::new(), true);
        }
        keep -= over;
    }
}

/// Render documents as one markdown message. Used when context is pushed into a *running* session
/// (refresh, provider switch), where the only channel to the provider is a user turn.
pub fn render_documents(documents: &[ContextDoc]) -> String {
    let mut rendered = String::from("# Project context\n\n");
    for document in documents {
        rendered.push_str(&format!(
            "## {}\n\n<!-- id: {} -->\n\n{}\n\n",
            document.title,
            document.id,
            document.body.trim_end()
        ));
    }
    rendered
}

/// Store failures are I/O from the command surface's point of view.
fn stored<T>(result: Result<T, StoreError>) -> IpcResult<T> {
    result.map_err(|error| IpcError::io(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ErrorCode;
    use crate::ipc::Session;
    use crate::plan::PlanUpdate;
    use crate::sessions::event::SessionState;
    use crate::sessions::event::TitleSource;
    use std::path::Path;

    fn doc(id: &str, body: &str) -> ContextDoc {
        ContextDoc {
            id: id.into(),
            title: format!("Title {id}"),
            body: body.into(),
        }
    }

    #[test]
    fn documents_under_budget_pass_through_untouched() {
        let input = vec![doc("a", "alpha"), doc("b", "bravo")];
        let assembly = assemble(&input, ContextLimits::default());

        assert_eq!(assembly.docs, input);
        assert_eq!(assembly.omitted, 0);
        assert_eq!(assembly.total_chars, 10);
        assert_eq!(
            assembly.documents,
            vec![
                ContextDocument {
                    id: "a".into(),
                    title: "Title a".into(),
                    chars: 5,
                    truncated: false,
                },
                ContextDocument {
                    id: "b".into(),
                    title: "Title b".into(),
                    chars: 5,
                    truncated: false,
                },
            ]
        );
    }

    #[test]
    fn a_document_over_the_per_document_budget_is_truncated_with_an_explicit_marker() {
        let body: String = "x".repeat(500);
        let limits = ContextLimits {
            per_doc_chars: 200,
            total_chars: 10_000,
        };
        let assembly = assemble(&[doc("long", &body)], limits);

        let rendered = &assembly.docs[0].body;
        let chars = rendered.chars().count();
        assert!(chars <= 200, "the budget is a hard cap: {chars}");
        assert!(
            rendered.starts_with(&"x".repeat(MIN_DOC_CHARS)),
            "the head of the document is kept"
        );
        assert!(
            rendered.ends_with(" chars omitted]"),
            "the marker is explicit and at the end: {rendered:?}"
        );
        assert!(rendered.contains("[truncated: "));
        assert_eq!(
            assembly.documents[0].chars, chars,
            "the record counts the marker too"
        );
        assert!(assembly.documents[0].truncated);
        assert_eq!(assembly.total_chars, chars);
    }

    #[test]
    fn the_total_budget_caps_the_whole_assembly_and_counts_omitted_documents() {
        let body: String = "y".repeat(150);
        let limits = ContextLimits {
            per_doc_chars: 200,
            total_chars: 400,
        };
        let assembly = assemble(
            &[
                doc("a", &body),
                doc("b", &body),
                doc("c", &body),
                doc("d", &body),
            ],
            limits,
        );

        assert!(
            assembly.total_chars <= 400,
            "the total budget is a hard cap: {}",
            assembly.total_chars
        );
        assert_eq!(
            assembly.documents.len() + assembly.omitted,
            4,
            "every input is either kept or reported as omitted"
        );
        assert!(assembly.omitted >= 1, "the budget cannot hold all four");
        assert_eq!(
            assembly
                .documents
                .iter()
                .map(|info| info.id.as_str())
                .collect::<Vec<_>>(),
            assembly
                .docs
                .iter()
                .map(|doc| doc.id.as_str())
                .collect::<Vec<_>>(),
            "the record and the documents stay in step"
        );
        let counted: usize = assembly.documents.iter().map(|info| info.chars).sum();
        assert_eq!(counted, assembly.total_chars);
    }

    #[test]
    fn a_document_that_cannot_fit_a_marker_is_omitted_rather_than_mangled() {
        let limits = ContextLimits {
            per_doc_chars: 80,
            total_chars: 80,
        };
        let assembly = assemble(
            &[doc("a", &"z".repeat(40)), doc("b", &"z".repeat(40))],
            limits,
        );

        assert_eq!(assembly.documents.len(), 1);
        assert_eq!(assembly.omitted, 1);
        assert!(!assembly.documents[0].truncated);
    }

    #[test]
    fn an_empty_context_is_empty_not_an_error() {
        let assembly = assemble(&[], ContextLimits::default());
        assert!(assembly.docs.is_empty());
        assert_eq!(assembly.total_chars, 0);
    }

    // ---------- the service, against real spec files and a real plan mirror ----------

    fn write_spec(root: &Path, file: &str, id: &str, title: &str, body: &str) {
        let source = format!("---\nid: {id}\ntype: module-design\ntitle: {title}\n---\n\n{body}\n");
        std::fs::write(root.join(file), source).unwrap();
    }

    fn session_row(id: &str, project: &Path) -> Session {
        Session {
            id: id.into(),
            provider: "mock".into(),
            cwd: crate::paths::display_path(project),
            model: None,
            title: "Knowledge".into(),
            status: SessionState::Idle,
            project_id: None,
            awaiting: false,
            archived_at: None,
            title_source: TitleSource::Manual,
            created_at: "2025-01-01T00:00:00.000Z".into(),
            updated_at: "2025-01-01T00:00:00.000Z".into(),
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        store: Arc<Store>,
        plan: Arc<PlanService>,
        service: SessionContextService,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        store
            .insert_session(
                &session_row("session-1", dir.path()),
                &serde_json::json!({}),
            )
            .unwrap();
        let plan = Arc::new(PlanService::new(
            Arc::clone(&store),
            dir.path().to_path_buf(),
        ));
        let service = SessionContextService::new(
            Arc::clone(&store),
            Arc::clone(&plan),
            dir.path().to_path_buf(),
        );
        write_spec(
            dir.path(),
            "a.md",
            "spec-a",
            "Spec A",
            "# Spec A\n\nThe first pinned spec.",
        );
        write_spec(
            dir.path(),
            "b.md",
            "spec-b",
            "Spec B",
            "# Spec B\n\nThe second pinned spec.",
        );
        Fixture {
            _dir: dir,
            store,
            plan,
            service,
        }
    }

    #[test]
    fn a_snapshot_is_the_pins_in_pin_order_followed_by_the_plan() {
        let fixture = fixture();
        fixture.store.add_pin("session-1", "spec-b").unwrap();
        fixture.store.add_pin("session-1", "spec-a").unwrap();

        let empty = fixture.service.snapshot("session-1").unwrap();
        assert_eq!(empty.session_id, "session-1");
        assert_eq!(empty.provider, "mock");
        assert_eq!(empty.plan_version, 0);
        assert_eq!(
            empty
                .documents
                .iter()
                .map(|info| info.id.as_str())
                .collect::<Vec<_>>(),
            vec!["spec-b", "spec-a"],
            "the plan is absent because the session has no plan yet"
        );
        assert_eq!(empty.pins.len(), 2);
        assert_eq!(empty.pins[0].spec_id, "spec-b");
        assert_eq!(empty.pins[0].title, "Spec B");
        assert_eq!(empty.limits, ContextLimits::default());

        let docs = fixture.service.assembly("session-1").unwrap().docs;
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].id, "spec-b");
        assert_eq!(docs[0].title, "Spec B");
        assert_eq!(docs[0].body, "\n# Spec B\n\nThe second pinned spec.\n");
    }

    #[test]
    fn the_plan_is_the_last_document_and_carries_its_version() {
        let fixture = fixture();
        fixture
            .plan
            .set(
                "session-1",
                PlanUpdate::new("# Plan\n\n- ship ticket 14\n", 0),
            )
            .unwrap();

        let snapshot = fixture.service.snapshot("session-1").unwrap();
        assert_eq!(snapshot.plan_version, 1);
        assert_eq!(snapshot.documents.len(), 1);
        assert_eq!(snapshot.documents[0].id, PLAN_DOC_ID);
        assert_eq!(snapshot.documents[0].title, "Plan");
        assert!(fixture
            .service
            .assembly("session-1")
            .unwrap()
            .docs
            .first()
            .expect("the plan document")
            .body
            .contains("ship ticket 14"));
    }

    #[test]
    fn pinning_an_unknown_spec_is_rejected_and_pinning_is_idempotent() {
        let fixture = fixture();

        let error = fixture
            .service
            .pin_spec("session-1", "not-a-spec")
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Invalid);

        let first = fixture.service.pin_spec("session-1", "spec-a").unwrap();
        let again = fixture.service.pin_spec("session-1", "spec-a").unwrap();
        assert_eq!(first.pins, again.pins);
        assert_eq!(again.documents.len(), 1);

        let unpinned = fixture.service.unpin_spec("session-1", "spec-a").unwrap();
        assert!(unpinned.pins.is_empty());
        assert!(unpinned.documents.is_empty());
    }

    #[test]
    fn a_pin_whose_spec_disappeared_keeps_its_entry_and_contributes_nothing() {
        let fixture = fixture();
        fixture.store.add_pin("session-1", "spec-a").unwrap();
        std::fs::remove_file(fixture._dir.path().join("a.md")).unwrap();

        let snapshot = fixture.service.snapshot("session-1").unwrap();
        assert_eq!(snapshot.pins.len(), 1);
        assert_eq!(
            snapshot.pins[0].title, "spec-a",
            "a stale pin is still listed by id"
        );
        assert!(snapshot.documents.is_empty());
    }

    #[test]
    fn the_snapshot_of_a_missing_session_is_not_found() {
        let fixture = fixture();
        let error = fixture.service.snapshot("missing").unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }
}
