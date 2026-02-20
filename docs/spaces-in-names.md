# Plan: Allow Spaces in Agent Names

## Goal

Allow users to enter agent names with spaces (e.g., "space in name"), and automatically convert spaces to hyphens for the branch name (e.g., "space-in-name").

## Current Behavior

- User enters a single string in `InputMode::NewAgent` modal
- The same string is used for both `name` and `branch` (see `src/main.rs:1642-1644`)
- Branch names with spaces would cause git worktree creation to fail

## Implementation Plan

### 1. Create a branch name sanitization utility

**File:** `src/util/mod.rs` (new file) or add to existing utility module

```rust
/// Convert a name with spaces to a valid git branch name.
/// - Spaces become hyphens
/// - Multiple spaces become single hyphen
/// - Leading/trailing spaces are trimmed
pub fn sanitize_branch_name(name: &str) -> String {
    name.trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}
```

**Decision:** Lowercase branch names. "Space In Name" → "space-in-name"

### 2. Update the agent creation flow

**File:** `src/main.rs` (~line 1639-1646)

Change from:
```rust
InputMode::NewAgent => {
    if !input.is_empty() {
        action_tx.send(Action::CreateAgent {
            name: input.clone(),
            branch: input,
        })?;
    }
}
```

To:
```rust
InputMode::NewAgent => {
    if !input.is_empty() {
        let branch = sanitize_branch_name(&input);
        action_tx.send(Action::CreateAgent {
            name: input.trim().to_string(),
            branch,
        })?;
    }
}
```

### 3. Update the UI prompt (optional)

**File:** `src/ui/app.rs` (~line 156)

Current: `"Enter branch name:"`

Could change to: `"Enter agent name (spaces allowed):"` or `"Enter name:"`

This clarifies that spaces are permitted and the user is naming the agent, not directly naming the branch.

### 4. Add validation (optional enhancement)

Consider adding validation to show an error if the sanitized branch name would be empty after processing (e.g., user enters only spaces).

```rust
if branch.is_empty() {
    action_tx.send(Action::ShowError("Invalid name".to_string()))?;
    return Ok(());
}
```

### 5. Update tests

Add unit tests for `sanitize_branch_name`:
- `"space in name"` → `"space-in-name"`
- `"  space  in  name  "` → `"space-in-name"`
- `"Space In Name"` → `"space-in-name"` (if lowercasing)
- `"single"` → `"single"`
- `"   "` → `""` (empty, should be rejected)

## Files to Modify

| File | Change |
|------|--------|
| `src/util/mod.rs` | **New** - Add `sanitize_branch_name` function |
| `src/lib.rs` | Export `util` module |
| `src/main.rs` | Import and use sanitization in `SubmitInput` handler |
| `src/ui/app.rs` | (Optional) Update modal prompt from "Enter branch name" to "Enter name" |

## Decisions

1. **Lowercase branch names:** Yes - "Space In Name" → "space-in-name"
2. **Location:** New `src/util/mod.rs` module for helper functions

## Testing the Change

1. Create agent with name "test feature"
2. Verify agent name shows as "test feature" in UI
3. Verify branch is "test-feature"
4. Verify worktree path is `~/.flock/worktrees/<id>/test-feature`
