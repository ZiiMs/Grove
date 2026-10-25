# Domain docs

Single-context repository.

## Layout

- `GLOSSARY.md` at the repo root — the project's domain vocabulary.
- `docs/adr/` — one file per architecture decision record, `NNNN-<slug>.md`.
- `architecture.md` — the current architecture design; the ADRs record *why* it changed over time.

`GLOSSARY.md` and `docs/adr/` are populated. Add a term the first time a concept needs a definition,
and an ADR only when a decision is expensive to reverse or contradicts a previous one. Do not create
empty ones.

## Consumer rules

- **Before naming something** in code, a ticket, or a spec, read `GLOSSARY.md`. Use its terms exactly;
  if you need a new term, add it there in the same change rather than inventing a synonym.
- **Before an architectural change**, read `architecture.md` and the ADRs that touch the area. An ADR
  that contradicts `architecture.md` is stale — reconcile the two rather than following either blindly.
- **When a change alters a boundary, contract, or invariant**, update `architecture.md` in the same
  change, and add an ADR if the decision will be expensive to undo.
- Keep the wording plain. A glossary entry is a definition, not marketing copy.
