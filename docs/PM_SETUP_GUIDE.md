# Implementing PM Setup Wizards

This guide explains how to add a guided setup wizard for a Project Management provider (e.g., Notion, ClickUp, Airtable, Asana) following the Linear implementation pattern.

## Overview

Each PM setup wizard is a 3-step modal that guides users through:
1. **Token Setup** - Instructions for obtaining and setting the API token
2. **Team/Project Selection** - Fetch options from API and let user select
3. **Advanced Settings** (collapsible) - Optional configuration for IDs and states

## Implementation Steps

### 1. Add State Structures (`src/app/state.rs`)

The `PmSetupState` is shared across all providers. No new state struct needed - just use the existing one:

```rust
// Already exists in state.rs
pub struct PmSetupState {
    pub active: bool,
    pub step: PmSetupStep,
    pub advanced_expanded: bool,
    pub teams: Vec<(String, String, String)>,  // (id, name, key) - adapt as needed
    pub teams_loading: bool,
    pub selected_team_index: usize,
    pub manual_team_id: String,
    pub in_progress_state: String,
    pub done_state: String,
    pub dropdown_open: bool,
    pub dropdown_index: usize,
    pub field_index: usize,
    pub error: Option<String>,
}
```

### 2. Add Client Fetch Method

In your PM client (e.g., `src/notion/client.rs`), add a method to fetch selectable options:

```rust
impl NotionClient {
    /// Fetch databases the user has access to
    pub async fn fetch_databases(&self) -> Result<Vec<(String, String)>> {
        let token = self.token.as_ref().context("Notion token not set")?;
        
        let response = self.client
            .post("https://api.notion.com/v1/databases/query")
            .header("Authorization", format!("Bearer {}", token))
            .header("Notion-Version", "2022-06-28")
            .json(&json!({ "page_size": 100 }))
            .send()
            .await?
            .json::<NotionDatabasesResponse>()
            .await?;
        
        Ok(response.results.into_iter()
            .map(|db| (db.id, db.title.first().map(|t| t.plain_text.clone()).unwrap_or_default()))
            .collect())
    }
}
```

### 3. Add Actions to Action Enum (`src/app/action.rs`)

The actions are already defined and shared. You may need to add provider-specific load actions:

```rust
// Already exists - reuse these:
Action::PmSetupTeamsLoaded { teams: Vec<(String, String, String)> },
Action::PmSetupTeamsError { message: String },
```

### 4. Update PM Setup Modal (`src/ui/components/pm_setup_modal.rs`)

Add a new render method for your provider:

```rust
impl<'a> PmSetupModal<'a> {
    fn render_content(&self, frame: &mut Frame, area: Rect) {
        let lines = match self.provider {
            ProjectMgmtProvider::Linear => self.render_linear_content(),
            ProjectMgmtProvider::Notion => self.render_notion_content(),  // Add this
            ProjectMgmtProvider::Clickup => self.render_clickup_content(),
            ProjectMgmtProvider::Airtable => self.render_airtable_content(),
            ProjectMgmtProvider::Asana => self.render_asana_content(),
        };
        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_notion_content(&self) -> Vec<Line<'static>> {
        match self.state.step {
            PmSetupStep::Token => self.render_notion_token_step(),
            PmSetupStep::Team => self.render_notion_database_step(),
            PmSetupStep::Advanced => self.render_notion_advanced_step(),
        }
    }

    fn render_notion_token_step(&self) -> Vec<Line<'static>> {
        let token_exists = std::env::var("NOTION_TOKEN").is_ok();
        let (status_symbol, status_color) = if token_exists {
            ("✓ OK", Color::Green)
        } else {
            ("✗ Missing", Color::Red)
        };

        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Notion uses an Integration Secret for authentication.",
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Go to: https://www.notion.so/my-integrations",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  2. Click \"+ New integration\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  3. Give it a name (e.g., \"Grove\")",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  4. Select the workspace and capabilities",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "  5. Copy the \"Internal Integration Secret\"",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            // Add provider-specific instructions
            Line::from(Span::styled(
                "  Note: Share your database with the integration in Notion!",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Add to your shell profile (~/.zshrc or ~/.bashrc):",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "    export NOTION_TOKEN=\"secret_your_token_here\"",
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Then restart Grove or run: source ~/.zshrc",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Token Status: ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("{} (NOTION_TOKEN)", status_symbol),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(""),
        ]
    }
}
```

### 5. Update Action Handlers (`src/main.rs`)

#### In `handle_pm_setup_key`:
No changes needed - key handling is provider-agnostic.

#### In `process_action`:

Update `Action::PmSetupNextStep` to fetch from your provider:

```rust
Action::PmSetupNextStep => {
    match state.pm_setup.step {
        PmSetupStep::Token => {
            state.pm_setup.step = PmSetupStep::Team;
            
            // Fetch based on current provider
            let provider = state.settings.repo_config.project_mgmt.provider;
            
            if state.pm_setup.teams.is_empty() {
                match provider {
                    ProjectMgmtProvider::Linear if Config::linear_token().is_some() => {
                        state.pm_setup.teams_loading = true;
                        let tx = action_tx.clone();
                        let client = Arc::clone(linear_client);
                        tokio::spawn(async move {
                            match client.get_teams().await {
                                Ok(teams) => { let _ = tx.send(Action::PmSetupTeamsLoaded { teams }); }
                                Err(e) => { let _ = tx.send(Action::PmSetupTeamsError { message: e.to_string() }); }
                            }
                        });
                    }
                    ProjectMgmtProvider::Notion if Config::notion_token().is_some() => {
                        state.pm_setup.teams_loading = true;
                        let tx = action_tx.clone();
                        let client = Arc::clone(notion_client);
                        tokio::spawn(async move {
                            match client.fetch_databases().await {
                                Ok(dbs) => {
                                    let teams: Vec<_> = dbs.into_iter()
                                        .map(|(id, name)| (id, name, String::new()))
                                        .collect();
                                    let _ = tx.send(Action::PmSetupTeamsLoaded { teams });
                                }
                                Err(e) => { let _ = tx.send(Action::PmSetupTeamsError { message: e.to_string() }); }
                            }
                        });
                    }
                    _ => {} // Token not set, will show error in UI
                }
            }
        }
        // ... rest unchanged
    }
}
```

Update `Action::PmSetupComplete` to save provider-specific config:

```rust
Action::PmSetupComplete => {
    let provider = state.settings.repo_config.project_mgmt.provider;
    let manual_id = state.pm_setup.manual_team_id.clone();
    let in_progress = state.pm_setup.in_progress_state.clone();
    let done = state.pm_setup.done_state.clone();

    // Get selected ID (from dropdown or manual entry)
    let selected_id = if !manual_id.is_empty() {
        Some(manual_id)
    } else {
        state.pm_setup.teams.get(state.pm_setup.selected_team_index)
            .map(|t| t.0.clone())
    };

    if let Some(id) = selected_id {
        // Save to provider-specific config
        match provider {
            ProjectMgmtProvider::Linear => {
                state.settings.repo_config.project_mgmt.linear.team_id = Some(id);
                if !in_progress.is_empty() {
                    state.settings.repo_config.project_mgmt.linear.in_progress_state = Some(in_progress);
                }
                if !done.is_empty() {
                    state.settings.repo_config.project_mgmt.linear.done_state = Some(done);
                }
                linear_client.reconfigure(Config::linear_token().as_deref(), Some(id));
            }
            ProjectMgmtProvider::Notion => {
                state.settings.repo_config.project_mgmt.notion.database_id = Some(id);
                if !in_progress.is_empty() {
                    state.settings.repo_config.project_mgmt.notion.in_progress_option = Some(in_progress);
                }
                if !done.is_empty() {
                    state.settings.repo_config.project_mgmt.notion.done_option = Some(done);
                }
                notion_client.reconfigure(Config::notion_token().as_deref(), Some(id));
            }
            // ... other providers
        }

        if let Err(e) = state.settings.repo_config.save(&state.repo_path) {
            state.log_error(format!("Failed to save config: {}", e));
        } else {
            let name = state.pm_setup.teams.get(state.pm_setup.selected_team_index)
                .map(|t| t.1.clone())
                .unwrap_or_else(|| "manual".to_string());
            state.log_info(format!("{} setup complete: {}", provider.display_name(), name));
        }
    }

    state.pm_setup.active = false;
    state.settings.active = true;
}
```

### 6. Add Token Helper to Config (`src/app/config.rs`)

Add a helper method to check for your provider's token:

```rust
impl Config {
    pub fn notion_token() -> Option<String> {
        std::env::var("NOTION_TOKEN").ok()
    }

    pub fn clickup_token() -> Option<String> {
        std::env::var("CLICKUP_TOKEN").ok()
    }

    pub fn airtable_token() -> Option<String> {
        std::env::var("AIRTABLE_TOKEN").ok()
    }
}
```

## Checklist for New Provider

- [ ] Add `render_<provider>_content()` method to `PmSetupModal`
- [ ] Add `render_<provider>_token_step()` with token instructions
- [ ] Add `render_<provider>_selection_step()` for team/project/database selection
- [ ] Add fetch method to client (e.g., `fetch_databases()`, `fetch_lists()`)
- [ ] Add token helper to `Config` (e.g., `Config::notion_token()`)
- [ ] Update `Action::PmSetupNextStep` to fetch from your API
- [ ] Update `Action::PmSetupComplete` to save to your config section
- [ ] Add reconfigure method to client if needed

## Token URL Reference

| Provider | Token URL |
|----------|-----------|
| Linear | `https://linear.app/settings/account/security` |
| Notion | `https://www.notion.so/my-integrations` |
| ClickUp | `https://app.clickup.com/settings/apps` |
| Airtable | `https://airtable.com/create/tokens` |
| Asana | `https://app.asana.com/app/asana/-/account_management/developer_app` |

## Environment Variables

| Provider | Env Var | Token Prefix |
|----------|---------|--------------|
| Linear | `LINEAR_TOKEN` | `lin_api_` |
| Notion | `NOTION_TOKEN` | `secret_` |
| ClickUp | `CLICKUP_TOKEN` | `pk_` |
| Airtable | `AIRTABLE_TOKEN` | `pat` |
| Asana | `ASANA_TOKEN` | (none) |

## Testing

1. Open Grove in a test repo
2. Press `s` to open settings
3. Go to "Project Mgmt" tab
4. Select your provider from dropdown
5. Click "Setup <Provider>..."
6. Follow the wizard steps
7. Verify `.grove/project.toml` is updated correctly
