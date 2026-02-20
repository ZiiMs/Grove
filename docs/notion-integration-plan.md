# Notion Integration Plan

Integrate Notion as a project management backend following the existing Asana patterns.

## Overview

The integration will:
- Link existing Notion pages (tasks) to agents via URL/ID
- Auto-detect status property with config overrides
- Update status as agents progress
- Append completion notes to pages

---

## File Structure

```
src/notion/
├── mod.rs           # Re-exports
├── types.rs         # NotionTaskStatus, API response structs
└── client.rs        # NotionClient, OptionalNotionClient
```

---

## Phase 1: Core Types & Client

### 1.1 Types (`src/notion/types.rs`)

```rust
pub enum NotionTaskStatus {
    #[default]
    None,
    NotStarted { id: String, title: String, url: String },
    InProgress { id: String, title: String, url: String },
    Completed { id: String, title: String },
    Error { id: String, message: String },
}

// API response types
struct NotionPageResponse { ... }
struct NotionDatabaseResponse { ... }
struct NotionStatusProperty { id, name, status: { options: Vec<StatusOption> } }
```

### 1.2 Client (`src/notion/client.rs`)

- Base URL: `https://api.notion.com/v1/`
- Auth: Bearer token + `Notion-Version` header
- Methods:
  - `get_page(id)` - Fetch page details
  - `get_database_schema(database_id)` - Fetch status property options
  - `update_page_status(page_id, status_option_id)` - Update status
  - `append_page_content(page_id, blocks)` - Add completion notes

---

## Phase 2: Configuration

### 2.1 Global Config (`~/.flock/config.toml`)

```toml
[notion]
refresh_secs = 120  # Polling interval
```

### 2.2 Repo Config (`.flock/project.toml`)

```toml
[notion]
database_id = "abc123..."           # Optional: for status option discovery
status_property_name = "Status"      # Override property name (default: auto-detect)
in_progress_option = "In Progress"   # Override option name
done_option = "Done"                 # Override option name
```

### 2.3 Environment Variables

- `NOTION_TOKEN` - Notion integration secret (from https://www.notion.so/my-integrations)

---

## Phase 3: Agent Model & Actions

### 3.1 Update Agent Model (`src/agent/model.rs`)

```rust
pub struct Agent {
    // ... existing fields ...
    #[serde(default)]
    pub notion_task_status: NotionTaskStatus,
}
```

### 3.2 Actions (`src/app/action.rs`)

```rust
pub enum Action {
    // Notion operations
    AssignNotionTask { id: Uuid, url_or_id: String },
    UpdateNotionTaskStatus { id: Uuid, status: NotionTaskStatus },
    OpenNotionInBrowser { id: Uuid },
    DeleteAgentAndCompleteNotion { id: Uuid },
    // ...
}
```

### 3.3 Input Modes

```rust
pub enum InputMode {
    // ... existing ...
    AssignNotion,
    ConfirmDeleteNotion,
}
```

---

## Phase 4: Main.rs Integration

### 4.1 Client Initialization
- Create `OptionalNotionClient` with token from env
- Read repo config for database_id and status mapping

### 4.2 Status Auto-Discovery
- On first task assignment to a database, fetch schema
- Cache status property options in memory
- Map: "not started" variants → `NotStarted`, "in progress" variants → `InProgress`, "done" variants → `Completed`

### 4.3 Background Polling
- Similar to Asana polling task
- Poll tracked pages every `refresh_secs`
- Dispatch `UpdateNotionTaskStatus` on changes

### 4.4 Auto-transition on Agent Status Change
- When agent → `Running`: set Notion status to `InProgress`
- When agent → `Completed`/deleted: set status to `Completed`, append notes

### 4.5 URL/ID Parser
Support formats:
- `https://www.notion.so/Page-Title-UUID`
- `https://www.notion.so/UUID`
- Bare UUID (with or without dashes)

---

## Phase 5: Notes Sync

### 5.1 Completion Notes Format
When agent completes, append to page:

```markdown
## Agent Completed - [timestamp]

**Branch:** feature/xyz
**Task:** Task description

### Summary
[Agent output summary]

### Key Changes
- Change 1
- Change 2
```

### 5.2 Block Types
Use Notion's `heading_2`, `paragraph`, `bulleted_list_item` blocks

---

## Phase 6: UI Updates

### 6.1 Agent List Column
- Add "Notion" column (parallel to Asana)
- Show status indicator with color coding

### 6.2 Status Bar
- Add `[n] notion` shortcut

### 6.3 Settings Modal
- "Project Mgmt" tab: add Notion config fields
- Show `NOTION_TOKEN` status indicator

### 6.4 Project Setup Wizard
- Add `NotionDatabaseId` field

---

## Phase 7: Storage & Persistence

- `NotionTaskStatus` serialized with Agent in session file
- Uses `#[serde(default)]` for backward compatibility

---

## Implementation Order

| Step | Files | Description |
|------|-------|-------------|
| 1 | `src/notion/mod.rs`, `types.rs`, `client.rs` | Core types and API client |
| 2 | `src/app/config.rs` | Config structs |
| 3 | `src/agent/model.rs` | Add notion field to Agent |
| 4 | `src/app/action.rs` | Add Notion actions |
| 5 | `src/main.rs` | Client init, polling, action handling |
| 6 | `src/ui/components/agent_list.rs` | Notion column |
| 7 | `src/ui/components/status_bar.rs` | Keyboard shortcut |
| 8 | `src/ui/components/settings_modal.rs` | Config UI |
| 9 | `src/ui/components/project_setup.rs` | Setup wizard |

---

## Key Differences from Asana

| Aspect | Asana | Notion |
|--------|-------|--------|
| Status model | Sections (fixed) | Status property (flexible) |
| Status discovery | Explicit GIDs | Auto-detect from schema |
| Page content | No write | Append completion notes |
| URL format | Multiple patterns | Single pattern with UUID |

---

## API Reference

- Base URL: `https://api.notion.com/v1/`
- Auth: `Authorization: Bearer {token}`, `Notion-Version: 2022-06-28`
- Key endpoints:
  - `GET /pages/{page_id}` - Retrieve page
  - `GET /databases/{database_id}` - Retrieve database schema
  - `PATCH /pages/{page_id}` - Update page properties
  - `PATCH /blocks/{block_id}/children/append` - Append content blocks
