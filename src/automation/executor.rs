use anyhow::Result;
use tracing::{debug, warn};

use crate::app::config::{AutomationActionType, AutomationConfig};
use crate::core::projects::asana::OptionalAsanaClient;

pub async fn execute_automation(
    asana_client: &OptionalAsanaClient,
    config: &AutomationConfig,
    action_type: AutomationActionType,
    task_gid: &str,
    in_progress_override_gid: Option<&str>,
    done_override_gid: Option<&str>,
) -> Result<()> {
    let status_name = match action_type {
        AutomationActionType::TaskAssign => &config.on_task_assign,
        AutomationActionType::Push => &config.on_push,
        AutomationActionType::Delete => &config.on_delete,
    };

    let status = match status_name {
        Some(s) if !s.is_empty() => s,
        _ => {
            debug!("No automation configured for {:?}", action_type);
            return Ok(());
        }
    };

    debug!(
        "Executing automation: {:?} -> {} for task {}",
        action_type, status, task_gid
    );

    let lower = status.to_lowercase();

    if lower == "none" {
        debug!("Automation set to 'None', skipping");
        return Ok(());
    }

    if lower == "completed" || lower == "done" {
        asana_client.complete_task(task_gid).await?;
        if let Some(done_gid) = done_override_gid {
            let _ = asana_client.move_to_done(task_gid, Some(done_gid)).await;
        } else {
            let _ = asana_client.move_to_done(task_gid, None).await;
        }
        debug!("Automation: marked task {} as completed", task_gid);
        return Ok(());
    }

    if lower.contains("in progress") {
        asana_client
            .move_to_in_progress(task_gid, in_progress_override_gid)
            .await?;
        debug!("Automation: moved task {} to In Progress", task_gid);
        return Ok(());
    }

    if lower.contains("not started") || lower.contains("todo") || lower.contains("to do") {
        asana_client.move_to_not_started(task_gid, None).await?;
        debug!("Automation: moved task {} to Not Started", task_gid);
        return Ok(());
    }

    let sections = match asana_client.get_sections().await {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to fetch sections for automation: {}", e);
            return Err(e);
        }
    };

    for section in sections {
        if section.name.eq_ignore_ascii_case(status) {
            asana_client
                .move_task_to_section(task_gid, &section.gid)
                .await?;
            debug!(
                "Automation: moved task {} to section {}",
                task_gid, section.name
            );
            return Ok(());
        }
    }

    warn!("Automation: could not find section matching '{}'", status);
    Ok(())
}
