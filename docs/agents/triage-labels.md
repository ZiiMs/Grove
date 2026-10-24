# Triage labels

The five canonical triage roles and the label string each maps to. Defaults are kept as-is.

| Role | Label string | Meaning |
|---|---|---|
| Needs triage | `needs-triage` | New; not yet assessed |
| Needs info | `needs-info` | Blocked on a question for the reporter |
| Ready for agent | `ready-for-agent` | Well-scoped; an agent can take it |
| Ready for human | `ready-for-human` | Needs human judgement or credentials |
| Wontfix | `wontfix` | Deliberately not doing it |

Locally the label is written as the `Status:` line in the ticket file. Triage applies the existing
strings above rather than inventing new ones.
