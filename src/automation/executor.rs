use anyhow::Result;
use tracing::{debug, warn};

use crate::app::config::{AutomationActionType, AutomationConfig};
use crate::core::projects::asana::OptionalAsanaClient;

pub async fn execute_automation(
    asana_client: &OptionalAsanaClient,
    config: &AutomationConfig,
    action_type: AutomationActionType,
    task_gid: &str,
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

    let sections = match asana_client.get_sections().await {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to fetch sections for automation: {}", e);
            return Err(e);
        }
    };

    for section in &sections {
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
