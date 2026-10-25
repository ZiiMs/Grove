# Triage labels

The five canonical triage state roles, and the ClickUp tag each one maps to. `triage` applies these
strings as they are rather than inventing near-duplicates; a tag missing from `Team Space` is created
with `clickup_create_space_tag` before it is applied, never per ticket.

| Role | Tag | Meaning |
|---|---|---|
| Needs triage | `needs-triage` | New; not yet assessed |
| Needs info | `needs-info` | Blocked on a question for the reporter |
| Ready for agent | `ready-for-agent` | Well-scoped; an agent can take it |
| Ready for human | `ready-for-human` | Needs human judgement or credentials |
| Wontfix | `wontfix` | Deliberately not doing it |

Triage state is tags, not status. The list keeps its three statuses (`to do`, `in progress`,
`complete`) so status means "has work started and has it landed", and a ticket can be
`ready-for-agent` while still `to do`. A `wontfix` ticket gets the tag and is closed with status
`complete` — tickets are never deleted.

## Category tags

`triage` also asks for exactly one category per triaged ticket, alongside its one state tag:

| Role | Tag | Meaning |
|---|---|---|
| Bug | `bug` | Something is broken |
| Enhancement | `enhancement` | New feature or improvement |

Both are space tags like the state tags, created on first use. A ticket that has been through triage
carries one `bug`/`enhancement` tag and one state tag; an untriaged ticket carries neither.

Apply with `clickup_add_tag_to_task`, remove with `clickup_remove_tag_from_task` (both with
`workspace_id: "9017632039"`). A ticket carries at most one of the five state tags and at most one
category tag at a time.
