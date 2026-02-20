# Customizable Prompts Implementation Plan

## Overview
Add customizable prompts (Summary, Merge, Push) as project-specific settings in the General tab of the settings modal.

## Files to Modify

### 1. `src/app/config.rs`
- Add `PromptsConfig` struct with optional fields:
  - `summary_prompt: Option<String>`
  - `merge_prompt: Option<String>`
  - `push_prompt: Option<String>`
- Add `prompts: PromptsConfig` to `RepoConfig` struct
- Include default prompt values as functions for fallback

### 2. `src/app/state.rs`
- Add new `SettingsCategory::Prompts` variant
- Add new `SettingsField` variants: `SummaryPrompt`, `MergePrompt`, `PushPrompt`
- Update `SettingsItem::all_for_tab()` to include Prompts section in General tab
- Update `SettingsField::tab()` to route new fields to `SettingsTab::General`

### 3. `src/ui/components/settings_modal.rs`
- Add rendering for `SettingsCategory::Prompts`
- Add `get_field_display()` cases for `SummaryPrompt`, `MergePrompt`, `PushPrompt`
- Show truncated prompt preview with ellipsis for long values

### 4. `src/main.rs`
- Update `Action::RequestSummary` to use `repo_config.prompts.summary_prompt` or fallback to default
- Update `Action::MergeMain` to use `repo_config.prompts.merge_prompt` or fallback to default
- Update `Action::PushBranch` to use `repo_config.prompts.push_prompt` or fallback to agent-specific default

## Data Flow
```
RepoConfig (project.toml)
    └── prompts: PromptsConfig
            ├── summary_prompt: Option<String>
            ├── merge_prompt: Option<String>
            └── push_prompt: Option<String>
```

## Example `.flock/project.toml`
```toml
[prompts]
summary_prompt = "Please provide a brief summary..."
merge_prompt = "Merge main into this branch, resolve conflicts"
push_prompt = "Review changes and push to remote"
```

## UI Layout (General Tab)
```
── Agent ────────────────────────────────
    AI Agent      : Claude Code
    Log Level     : Info

── Storage ──────────────────────────────
    Worktree Loc  : Project directory

── Prompts ──────────────────────────────   ← NEW SECTION
    Summary       : Please provide a brief...
    Merge         : Please merge main...
    Push          : Review the changes...

── Display ──────────────────────────────
    Preview       : [x]
    Metrics       : [x]
    ...
```

## Default Prompts (for reference)

### Summary (main.rs:1315)
```
"Please provide a brief, non-technical summary of the work done on this branch. Format it as 1-5 bullet points suitable for sharing with non-technical colleagues on Slack. Focus on what was accomplished and why, not implementation details. Keep each bullet point to one sentence."
```

### Merge (main.rs:1224)
```
"Please merge {main_branch} into this branch. Handle any merge conflicts if they arise."
```

### Push (config.rs:54-63)
Agent-specific defaults:
- **Opencode**: "Review the changes, then commit and push them to the remote branch."
- **Codex**: "Please commit and push these changes"
- **Gemini**: "Please commit and push these changes"
- **Claude Code**: Uses `/push` command instead of prompt

## Implementation Notes

1. **Text editing**: Reuse existing `editing_text` and `text_buffer` pattern from `SettingsState`
2. **Long text handling**: Prompts can be long, ensure UI truncates with "..." for display
3. **Placeholder support**: Consider adding `{main_branch}` placeholder for merge prompt
4. **Validation**: Consider validating prompts aren't empty when set
5. **Backwards compatibility**: Empty/unset prompts should use defaults
